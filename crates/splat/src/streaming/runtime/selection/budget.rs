//! Distance-mode target budgeting adapted from deployed PlayCanvas dfcc50f's
//! GSplatBudgetBalancer / GSplatLodTable. MIT, Copyright (c) 2011-2026 PlayCanvas Ltd.
//! See docs/legal/playcanvas-MIT.txt. File residency/admission stays in selection.rs.

use super::{Manifest, View, intersects};

#[cfg(test)]
#[path = "camera_reference_tests.rs"]
mod camera_reference_tests;

const BINS: usize = 512;

/// Finest-to-coarsest retained levels (absolute LOD, leaf index, count).
struct Row {
    levels: Vec<(usize, usize, usize)>,
    coverage: f64,
}

fn bin(value: f64) -> usize {
    let lo = f64::from(1e-24_f32.to_bits());
    let hi = f64::from(1e3_f32.to_bits());
    ((f64::from((value as f32).to_bits()) - lo) * (BINS - 1) as f64 / (hi - lo))
        .clamp(0.0, (BINS - 1) as f64) as usize
}

/// Select targets independently of residency. Preserve the explicit
/// coarsest coverage floor for sparse tails (no implicit empty leaf).
pub(super) fn targets(manifest: &Manifest, view: View, budget: usize) -> Vec<Option<usize>> {
    let fov = if view.fov_scale.is_finite() && view.fov_scale > 0.0 {
        f64::from(view.fov_scale)
    } else {
        1.0
    };
    let rows: Vec<_> = manifest
        .leaves
        .iter()
        .enumerate()
        .map(|(id, leaf)| {
            if leaf.lods.is_empty() {
                return None;
            }
            // Source residency alone cannot cover a turn before the next LOD
            // tick. Keep one render-ready coarse range behind/outside the view;
            // only relevant leaves may spend the remaining budget on detail.
            let finest = if intersects(leaf.bound, view.clip_from_local) {
                view.quality
                    .and_then(|targets| targets.get(id))
                    .copied()
                    .unwrap_or(0)
                    .min(leaf.lods.len() - 1)
            } else {
                leaf.lods.len() - 1
            };
            let distance_sq =
                super::super::distance::full_bounds_squared(leaf.bound, view.eye, view.forward)
                    * fov
                    * fov;
            Some(Row {
                levels: leaf
                    .lods
                    .iter()
                    .enumerate()
                    .skip(finest)
                    .map(|(index, (lod, range))| (*lod, index, range.count))
                    .collect(),
                coverage: (1.0 / distance_sq.max(1e-12)).max(1e-12),
            })
        })
        .collect();
    allocate(&rows, budget)
}

/// Intrusive FIFO buckets: one live link per leaf, no allocation per upgrade.
struct Queue {
    head: [Option<usize>; BINS],
    tail: [Option<usize>; BINS],
    next: Vec<Option<usize>>,
}

impl Queue {
    fn new(len: usize) -> Self {
        Self {
            head: [None; BINS],
            tail: [None; BINS],
            next: vec![None; len],
        }
    }

    fn push(&mut self, bucket: usize, leaf: usize) {
        self.next[leaf] = None;
        if let Some(tail) = self.tail[bucket] {
            self.next[tail] = Some(leaf);
        } else {
            self.head[bucket] = Some(leaf);
        }
        self.tail[bucket] = Some(leaf);
    }

    fn pop(&mut self, bucket: usize) -> Option<usize> {
        let leaf = self.head[bucket]?;
        self.head[bucket] = self.next[leaf];
        if self.head[bucket].is_none() {
            self.tail[bucket] = None;
        }
        Some(leaf)
    }
}

// Manifest pruning guarantees strictly increasing counts toward finer detail.
// In distance mode the best-reachable ratio is exactly this boundary's 3² band
// weight: later, finer boundaries cannot offer a greater weighted average.
fn ratio(row: &Row, coarse: usize) -> f64 {
    f64::from(9.0_f64.powi(row.levels[coarse].0 as i32 - 1) as f32)
}

fn allocate(rows: &[Option<Row>], budget: usize) -> Vec<Option<usize>> {
    let mut chosen: Vec<_> = rows
        .iter()
        .map(|row| row.as_ref().and_then(|r| r.levels.len().checked_sub(1)))
        .collect();
    let total = |finest: bool| {
        rows.iter()
            .flatten()
            .filter_map(|r| {
                if finest {
                    r.levels.first()
                } else {
                    r.levels.last()
                }
            })
            .fold(0usize, |sum, level| sum.saturating_add(level.2))
    };
    let mut spent = total(false);
    if total(true) <= budget {
        for (row, choice) in rows.iter().zip(&mut chosen) {
            *choice = row
                .as_ref()
                .and_then(|r| r.levels.first().map(|level| level.1));
        }
        return chosen;
    }
    if spent < budget {
        let mut queue = Queue::new(rows.len());
        for (id, row) in rows.iter().enumerate() {
            if let Some(row) = row
                && row.levels.len() > 1
            {
                queue.push(bin(row.coverage * ratio(row, row.levels.len() - 1)), id);
            }
        }
        'drain: for bucket in (0..BINS).rev() {
            while let Some(id) = queue.pop(bucket) {
                let (Some(row), Some(coarse)) = (&rows[id], chosen[id]) else {
                    continue;
                };
                let fine = coarse - 1;
                let cost = row.levels[fine].2 - row.levels[coarse].2;
                if cost > budget - spent {
                    break 'drain;
                }
                spent += cost;
                chosen[id] = Some(fine);
                if fine > 0 {
                    // The reference caches coverage as f32 after the seed pass.
                    let value = f64::from(row.coverage as f32) * ratio(row, fine);
                    queue.push(bin(value).min(bucket), id);
                }
            }
        }
    }
    rows.iter()
        .zip(chosen)
        .map(|(row, choice)| row.as_ref().zip(choice).map(|(r, index)| r.levels[index].1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn quality_target_is_a_refinement_ceiling_not_a_budget_to_fill() {
        use bevy::prelude::*;
        let manifest = Manifest::parse(br#"{"lodLevels":3,"filenames":["f/meta.json","m/meta.json","c/meta.json"],
          "tree":{"bound":{"min":[-1,-1,-6],"max":[1,1,-4]},"lods":{
          "0":{"file":0,"offset":0,"count":100},"1":{"file":1,"offset":0,"count":25},"2":{"file":2,"offset":0,"count":4}}}}"#).expect("fixture");
        let quality = [1];
        let view = View {
            quality: Some(&quality),
            ..Vec3::ZERO.into()
        };
        assert_eq!(targets(&manifest, view, 1000), [Some(1)]);
        assert_eq!(targets(&manifest, view, 10), [Some(2)]);
        assert_eq!(targets(&manifest, Vec3::ZERO.into(), 1000), [Some(0)]);
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        counts: Vec<Vec<usize>>,
        distances: Vec<f64>,
        budget: usize,
        expected: Vec<usize>,
    }

    #[test]
    fn matches_executed_playcanvas_target_budget_fixtures() {
        // Executed from the deployed engine, not our allocation helper.
        // Regeneration: scripts/sog-deployed-budget-reference.ts --write-fixture.
        let fixture: Fixture = serde_json::from_slice(include_bytes!(
            "../fixtures/playcanvas-deployed-budget.json"
        ))
        .expect("valid reference fixture");
        for case in fixture.cases {
            let rows: Vec<_> = case
                .counts
                .iter()
                .zip(&case.distances)
                .map(|(counts, distance)| {
                    Some(Row {
                        levels: counts
                            .iter()
                            .copied()
                            .enumerate()
                            .map(|(index, count)| (index, index, count))
                            .collect(),
                        coverage: (1.0 / distance.max(1e-12)).max(1e-12),
                    })
                })
                .collect();
            assert_eq!(
                allocate(&rows, case.budget),
                case.expected.into_iter().map(Some).collect::<Vec<_>>(),
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn kitchen_targets_match_captured_deployed_supersplat_ranges() {
        use bevy::prelude::*;
        // Actual sorted AND stochastic browser selections, not a reimplementation
        // of the allocator: Viewer 1.36.2 / Engine dfcc50f, 2026-10-01T20:19Z.
        // Bundle SHA256: 812f77c0cf50420034d54de268fa1963d666a0a080d451bec1c7e022ad9e8fa6.
        // Preserved reference-camera selection evidence.
        // The checked-in manifest has identical parsed tree/files to that capture.
        let manifest = Manifest::parse(include_bytes!("../fixtures/sanatorium-lod.json"))
            .expect("kitchen manifest");
        let expected = [
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 5, 4, 5, 5, 5,
            5, 5, 5, 5, 5, 4, 4, 0, 3, 0, 4, 1, 4, 5, 5, 4, 4, 5, 5, 5, 5, 4, 4, 4, 4, 5, 5, 5, 5,
            5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
        ];
        // Reference angles are world-space PlayCanvas Euler degrees. Both use
        // right-handed Y-up cameras. The source root has a 180-degree Z rotation.
        let world_from_local = Quat::from_rotation_z(std::f32::consts::PI);
        let rotation = Quat::from_rotation_y(146.60005_f32.to_radians())
            * Quat::from_rotation_x(6.400005_f32.to_radians());
        let view = View {
            eye: world_from_local.inverse() * Vec3::new(2.0106425, -0.12738079, -6.3471293),
            forward: world_from_local.inverse() * (rotation * Vec3::NEG_Z),
            // 110 degrees on the longer (horizontal) axis, canvas 1280 x 757.
            fov_scale: (55_f32.to_radians()).tan()
                / (1280. / 757.)
                / std::f32::consts::FRAC_PI_8.tan(),
            clip_from_local: None,
            ..Vec3::ZERO.into()
        };
        // Environment is selected independently and spends 2,783 of the 1M cap.
        let chosen = targets(&manifest, view, 1_000_000 - 2_783);
        let actual: Vec<_> = manifest
            .leaves
            .iter()
            .zip(&chosen)
            .map(|(leaf, index)| leaf.lods[index.expect("all reference leaves selected")].0)
            .collect();
        assert_eq!(
            actual, expected,
            "absolute LOD per depth-first manifest leaf"
        );
        let selected: usize = manifest
            .leaves
            .iter()
            .zip(chosen)
            .map(|(leaf, index)| leaf.lods[index.expect("selected")].1.count)
            .sum();
        assert_eq!(selected + 2_783, 993_654);
        // Actual residency selection must converge to those same ranges when
        // all files fit. This is not a claim that the normal decoded cache does.
        let resident =
            super::super::select(&manifest, view, 1_000_000 - 2_783, usize::MAX, |_| true);
        for (id, (leaf, range)) in manifest.leaves.iter().zip(resident).enumerate() {
            let expected_range = leaf
                .lods
                .iter()
                .find(|(lod, _)| *lod == expected[id])
                .map(|(_, range)| *range);
            assert_eq!(range, expected_range, "resident selection leaf {id}");
        }
    }

    #[test]
    fn culled_rows_do_not_spend_the_budget() {
        let rows = [
            None,
            Some(Row {
                levels: vec![(0, 0, 8), (1, 1, 2)],
                coverage: 1.0,
            }),
            None,
        ];
        assert_eq!(allocate(&rows, 8), [None, Some(0), None]);
        assert!(allocate(&[], 8).is_empty());
    }
}
