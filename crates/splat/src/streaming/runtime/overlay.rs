//! Read-only evidence for displaying selected LOD bounds.
use super::{SogStream, SplatRange, manifest::Bounds};
use std::collections::BTreeSet;
/// Refinement state of an actually displayed region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefinementState {
    Settled,
    Loading,
    Capped,
}
#[derive(Default)]
pub(super) struct RefinementIntent {
    pub targets: Vec<Option<SplatRange>>,
    /// Actual admitted intermediate work, eligible/queued or in flight this tick.
    pub queued_targets: Vec<Option<SplatRange>>,
    pub capped_leaves: Vec<bool>,
    pub blocked_files: Vec<bool>,
    pub transition_blocked: bool,
}

fn range_key(range: &SplatRange) -> (usize, usize, usize) {
    (range.file, range.offset, range.count)
}

/// Describe displayed refinement from actual queue and ownership evidence.
pub(super) fn refinement_state(stream: &SogStream, leaf: usize, level: usize) -> RefinementState {
    let Some(leaf_data) = stream.manifest.leaves.get(leaf) else {
        return RefinementState::Settled;
    };
    let Some(Some(target)) = stream.overlay_intent.targets.get(leaf) else {
        return RefinementState::Settled;
    };
    let finer = |range: &SplatRange| {
        leaf_data
            .lods
            .iter()
            .any(|(lod, r)| *lod < level && r == range)
    };
    if !finer(target) {
        return RefinementState::Settled;
    }
    // Exact leaf ranges prevent an upload/shared source for another leaf from
    // making this leaf blink. Failed submissions are not preparation progress.
    if stream.submission_error.is_none() && stream.active.iter().chain(&stream.submitted).any(finer)
    {
        return RefinementState::Loading;
    }
    if let Some(Some(queued)) = stream.overlay_intent.queued_targets.get(leaf)
        && finer(queued)
        && let Some(entry) = stream.entries.get(queued.file)
    {
        if entry.loading.is_some() {
            return RefinementState::Loading;
        }
        if !stream.overlay_intent.transition_blocked
            && !stream
                .overlay_intent
                .blocked_files
                .get(queued.file)
                .copied()
                .unwrap_or(false)
            && entry.ready.is_none()
        {
            return RefinementState::Loading; // Admitted, queued behind concurrency.
        }
    }
    if stream
        .entries
        .get(target.file)
        .is_some_and(|entry| entry.ready.is_none() && entry.loading.is_none() && entry.attempts > 0)
    {
        return RefinementState::Settled; // Failed/backoff, not ongoing work.
    }
    let rejected = stream
        .overlay_intent
        .capped_leaves
        .get(leaf)
        .copied()
        .unwrap_or(false)
        || (finer(target)
            && (stream.overlay_intent.transition_blocked
                || stream
                    .overlay_intent
                    .blocked_files
                    .get(target.file)
                    .copied()
                    .unwrap_or(false)));
    if rejected
        && leaf_data.lods.iter().any(|(lod, range)| {
            *lod < level
                && stream
                    .entries
                    .get(range.file)
                    .is_some_and(|e| e.ready.is_some() || e.can_retry())
        })
    {
        RefinementState::Capped
    } else {
        RefinementState::Settled
    }
}

/// Return authored bounds and refinement evidence for displayed ranges.
pub fn displayed_bounds(stream: &SogStream) -> Vec<(Bounds, u32, RefinementState)> {
    let displayed: BTreeSet<_> = stream
        .displayed
        .iter()
        .filter(|r| r.count > 0)
        .map(range_key)
        .collect();
    stream
        .manifest
        .leaves
        .iter()
        .enumerate()
        .filter_map(|(id, leaf)| {
            leaf.lods
                .iter()
                .find(|(_, range)| displayed.contains(&range_key(range)))
                .map(|(level, _)| {
                    (
                        leaf.bound,
                        *level as u32,
                        refinement_state(stream, id, *level),
                    )
                })
        })
        .collect()
}

impl SogStream {
    /// Authored LOD levels for consumer legends.
    pub fn lod_levels(&self) -> impl Iterator<Item = u32> + '_ {
        self.manifest
            .leaves
            .iter()
            .flat_map(|leaf| leaf.lods.iter().map(|(level, _)| *level as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    #[test]
    fn bootstrap_coverage_is_not_evidence_of_a_budget_ceiling() {
        let mut stream = super::super::tests::tiny_stream();
        let coarse = stream.manifest.leaves[0].lods[0].1;
        stream.manifest.leaves[0].lods[0].0 = 3;
        stream.manifest.leaves[0]
            .lods
            .insert(0, (0, SplatRange { count: 8, ..coarse }));
        stream.overlay_intent.targets = vec![Some(coarse)];
        assert_eq!(refinement_state(&stream, 0, 3), RefinementState::Settled);
    }
    #[test]
    fn reached_budget_target_is_solid_even_when_coarser_than_lod_zero() {
        let mut stream = super::super::tests::tiny_stream();
        let coarse = stream.manifest.leaves[0].lods[0].1;
        stream.manifest.leaves[0].lods[0].0 = 3;
        stream.manifest.leaves[0]
            .lods
            .insert(0, (0, SplatRange { count: 8, ..coarse }));
        stream.displayed = vec![coarse];
        stream.tick(
            Vec3::ZERO.into(),
            0.,
            &super::super::SogLodSettings {
                splat_budget: 1,
                ..default()
            },
            100,
            None,
            |_, _, _, _| panic!("resident source"),
        );
        assert_eq!(refinement_state(&stream, 0, 3), RefinementState::Settled);
    }
    #[test]
    fn lod_wireframe_maps_exact_displayed_ranges_not_shared_files_or_requests() {
        let mut stream = super::super::tests::tiny_stream();
        let coarse = stream.manifest.leaves[0].lods[0].1;
        stream.manifest.leaves[0].lods[0].0 = 3;
        let fine = SplatRange {
            file: 1,
            offset: 0,
            count: 8,
        };
        stream.manifest.leaves[0].lods.insert(0, (0, fine));
        stream.displayed = vec![coarse];
        stream.active = vec![fine];
        stream.submitted = vec![fine];
        stream.overlay_intent.targets = vec![Some(fine), None];
        // A second leaf shares a file but not a range. It must not acquire a box.
        stream.manifest.leaves.push(super::super::manifest::Leaf {
            bound: super::super::manifest::Bounds {
                min: Vec3::ONE,
                max: Vec3::splat(2.),
            },
            lods: vec![(
                3,
                SplatRange {
                    file: 0,
                    offset: 1,
                    count: 1,
                },
            )],
        });
        let bounds = displayed_bounds(&stream);
        assert_eq!(bounds.len(), 1);
        assert_eq!(bounds[0].1, 3);
        assert_eq!(bounds[0].2, RefinementState::Loading);
        assert_eq!(bounds[0].0.min, Vec3::splat(-1.));
    }
}
