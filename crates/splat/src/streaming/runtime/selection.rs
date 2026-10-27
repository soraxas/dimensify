//! Camera-dependent LOD policy, independent of HTTP and ECS.

mod budget;

use bevy::prelude::*;
use std::collections::BinaryHeap;

use super::diagnostics::{self, MAX_LEAF_DIAGNOSTICS, SogLeafDiagnostic};
use super::manifest::{Bounds, Manifest, SplatRange};

pub(super) struct SourceAdmission<'a> {
    pub costs: &'a [usize],
    pub pinned: &'a [bool],
    pub capacity: usize,
}

/// Constant-size admission evidence, retained even when leaf tracing is off.
#[derive(Default, Clone, Copy)]
pub(super) struct AdmissionSummary {
    pub source_limited: bool,
    pub selection_limited: bool,
}

#[derive(Clone, Copy)]
pub(super) struct View<'a> {
    pub eye: Vec3,
    pub forward: Vec3,
    pub fov_scale: f32,
    pub clip_from_local: Option<Mat4>,
    pub pixels: Option<super::screen_space::PixelView>,
    /// Finest useful index per leaf. None preserves the reference allocator.
    pub quality: Option<&'a [usize]>,
}

impl From<Vec3> for View<'_> {
    fn from(eye: Vec3) -> Self {
        Self {
            eye,
            forward: Vec3::NEG_Z,
            fov_scale: 1.0,
            clip_from_local: None,
            pixels: None,
            quality: None,
        }
    }
}

/// Test the authored bound's circumsphere, as PlayCanvas's interval culler does.
/// Authored bounds include rotated/scaled extents, but are not guaranteed to
/// enclose the renderer's full cutoff or post-quantization support. The saved
/// kitchen audit confirms tails outside these boxes; see bounds_audit_tests.
/// The sphere preserves visible tails the tighter AABB excluded in the kitchen
/// pixel regression. It is not a proof of arbitrary producer support. Retain
/// our existing 10% clip margin and fail-open guards. Local-space plane normals
/// account for nonuniform/mirrored roots without assuming uniform world scale.
/// Homogeneous plane tests avoid dividing at
/// the near plane; eye-plane crossings fail open.
pub(super) fn intersects(bound: Bounds, clip_from_local: Option<Mat4>) -> bool {
    let Some(clip) = clip_from_local else {
        return true;
    };
    let determinant = clip.determinant();
    if !clip.is_finite()
        || !determinant.is_finite()
        || determinant == 0.0
        || !bound.min.is_finite()
        || !bound.max.is_finite()
        || !bound.min.cmple(bound.max).all()
    {
        return true;
    }
    let corners = std::array::from_fn::<_, 8, _>(|i| {
        let point = Vec3::new(
            if i & 1 == 0 { bound.min.x } else { bound.max.x },
            if i & 2 == 0 { bound.min.y } else { bound.max.y },
            if i & 4 == 0 { bound.min.z } else { bound.max.z },
        );
        clip * point.extend(1.0)
    });
    if corners.iter().any(|p| !p.is_finite() || p.w.abs() <= 1e-6)
        || (corners.iter().any(|p| p.w < 0.0) && corners.iter().any(|p| p.w > 0.0))
    {
        return true;
    }
    // With a consistent negative w, flipping all homogeneous components gives
    // the same NDC. This also rejects behind-eye reverse-Z perspective bounds.
    let sign = corners[0].w.signum();
    let rows = clip.transpose();
    let planes = [
        rows.w_axis * 1.1 + rows.x_axis,
        rows.w_axis * 1.1 - rows.x_axis,
        rows.w_axis * 1.1 + rows.y_axis,
        rows.w_axis * 1.1 - rows.y_axis,
        rows.z_axis,
        rows.w_axis - rows.z_axis,
    ];
    let center = bound.min * 0.5 + bound.max * 0.5;
    let radius = (bound.max * 0.5 - bound.min * 0.5).length();
    let support_distances = planes.map(|plane| {
        let plane = plane * sign;
        plane.dot(center.extend(1.0)) + radius * plane.truncate().length()
    });
    if support_distances.iter().any(|v| !v.is_finite()) {
        return true;
    }
    let tolerance = corners
        .iter()
        .map(|p| p.abs().max_element())
        .fold(1.0, f32::max)
        * 1e-5;
    !support_distances
        .iter()
        .any(|distance| *distance < -tolerance)
}

/// One entry per spatial leaf, never overlapping representations of a leaf.
#[cfg(test)]
pub(super) fn select<'a>(
    manifest: &Manifest,
    view: impl Into<View<'a>>,
    budget: usize,
    source_budget: usize,
    available: impl Fn(usize) -> bool,
) -> Vec<Option<SplatRange>> {
    let mut pinned = vec![false; manifest.filenames.len()];
    for file in manifest.coarse_files() {
        pinned[file] = true;
    }
    select_admitted(
        manifest,
        view.into(),
        budget,
        &[SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &pinned,
            capacity: source_budget,
        }],
        available,
        None,
    )
}

/// Charge the union of immutable transition pins and candidate sources separately
/// for each memory domain (padded decoded CPU rows versus logical GPU rows).
#[cfg(test)]
pub(super) fn select_admitted(
    manifest: &Manifest,
    view: View,
    budget: usize,
    admissions: &[SourceAdmission<'_>],
    available: impl Fn(usize) -> bool,
    trace: Option<(&mut Vec<SogLeafDiagnostic>, Option<Mat4>)>,
) -> Vec<Option<SplatRange>> {
    select_with_summary(
        manifest,
        view,
        budget,
        admissions,
        available,
        trace,
        &mut AdmissionSummary::default(),
    )
}

/// Select bounded detail and return admission evidence.
pub(super) fn select_with_summary(
    manifest: &Manifest,
    view: View,
    budget: usize,
    admissions: &[SourceAdmission<'_>],
    available: impl Fn(usize) -> bool,
    trace: Option<(&mut Vec<SogLeafDiagnostic>, Option<Mat4>)>,
    summary: &mut AdmissionSummary,
) -> Vec<Option<SplatRange>> {
    select_with_cap_evidence(
        manifest, view, budget, admissions, available, trace, summary, None,
    )
}

/// Optional lightweight per-leaf rejection evidence without text/projection
/// diagnostics. The returned selection is identical with or without evidence.
pub(super) fn target_ranges(
    manifest: &Manifest,
    view: View,
    budget: usize,
) -> Vec<Option<SplatRange>> {
    budget::targets(manifest, view, budget)
        .into_iter()
        .zip(&manifest.leaves)
        .map(|(index, leaf)| index.and_then(|i| leaf.lods.get(i).map(|(_, range)| *range)))
        .collect()
}

/// Select bounded detail and record which leaves hit a source ceiling.
pub(super) fn select_with_cap_evidence(
    manifest: &Manifest,
    view: View,
    budget: usize,
    admissions: &[SourceAdmission<'_>],
    available: impl Fn(usize) -> bool,
    mut trace: Option<(&mut Vec<SogLeafDiagnostic>, Option<Mat4>)>,
    summary: &mut AdmissionSummary,
    mut capped: Option<&mut Vec<bool>>,
) -> Vec<Option<SplatRange>> {
    *summary = AdmissionSummary::default();
    let targets = budget::targets(manifest, view, budget);
    summary.selection_limited = targets.iter().enumerate().any(|(id, index)| {
        intersects(manifest.leaves[id].bound, view.clip_from_local)
            && index.is_some_and(|index| {
                index > view.quality.and_then(|q| q.get(id)).copied().unwrap_or(0)
            })
    });
    if let Some(caps) = capped.as_mut() {
        caps.clear();
        caps.resize(targets.len(), false);
    }
    let mut chosen: Vec<_> = manifest
        .leaves
        .iter()
        .zip(&targets)
        .map(|(leaf, target)| {
            let target = (*target)?;
            leaf.lods
                .iter()
                .enumerate()
                .rposition(|(i, (_, r))| i >= target && (r.count == 0 || available(r.file)))
        })
        .collect();
    let mut refs = vec![0usize; manifest.filenames.len()];
    // Coarse files remain available when moving into an unloaded region.
    for file in manifest.coarse_files() {
        refs[file] += 1;
    }
    let mut count = 0usize;
    for (leaf, index) in manifest.leaves.iter().zip(&chosen) {
        if let Some(i) = index {
            let r = leaf.lods[*i].1;
            refs[r.file] += 1;
            count = count.saturating_add(r.count);
        }
    }
    let mut source_counts: Vec<usize> = admissions
        .iter()
        .map(|a| {
            a.costs
                .iter()
                .enumerate()
                .filter(|(i, _)| a.pinned[*i] || refs.get(*i).is_some_and(|n| *n > 0))
                .fold(0usize, |total, (_, cost)| total.saturating_add(*cost))
        })
        .collect();
    let mut reasons = trace
        .as_ref()
        .map(|_| vec!["selected target"; manifest.leaves.len().min(MAX_LEAF_DIAGNOSTICS)]);
    let mut heap = BinaryHeap::new();
    let candidate = |leaf_id: usize, index: usize| {
        let leaf = &manifest.leaves[leaf_id];
        let target = targets[leaf_id]?;
        let next = (target..index).rev().find(|&i| {
            let r = leaf.lods[i].1;
            r.count == 0 || available(r.file)
        })?;
        let adjusted = super::distance::full_bounds_squared(leaf.bound, view.eye, view.forward)
            * f64::from(view.fov_scale).powi(2);
        let score = 9.0_f64.powi(leaf.lods[index].0.saturating_sub(1) as i32) / adjusted.max(1e-12);
        Some((score.to_bits(), std::cmp::Reverse(leaf_id), next))
    };
    for (id, index) in chosen.iter().enumerate() {
        if let Some(i) = index
            && let Some(c) = candidate(id, *i)
        {
            heap.push(c);
        }
    }
    while let Some((score, std::cmp::Reverse(id), next)) = heap.pop() {
        let leaf = &manifest.leaves[id];
        let Some(old_index) = chosen[id] else {
            continue;
        };
        let old = leaf.lods[old_index].1;
        let new = leaf.lods[next].1;
        let Some(new_count) = (count - old.count).checked_add(new.count) else {
            continue;
        };
        let next_cost = |a: &SourceAdmission<'_>, total: usize| {
            if new.file == old.file {
                return total;
            }
            let total = if refs[old.file] == 1 && !a.pinned[old.file] {
                total.saturating_sub(a.costs[old.file])
            } else {
                total
            };
            if refs[new.file] == 0 && !a.pinned[new.file] {
                total.saturating_add(a.costs[new.file])
            } else {
                total
            }
        };
        let source_fits = admissions
            .iter()
            .zip(&source_counts)
            .all(|(a, total)| next_cost(a, *total) <= a.capacity);
        if new_count > budget || !source_fits {
            if let Some(caps) = capped.as_mut() {
                caps[id] = true;
            }
            summary.source_limited |= !source_fits;
            summary.selection_limited |= new_count > budget;
            if let Some(reason) = reasons.as_mut().and_then(|r| r.get_mut(id)) {
                *reason = if !source_fits {
                    "source transition"
                } else {
                    "device/slot cap"
                };
            }
            // A cached intermediate can require an extra source while an
            // already-visible finer source is in the immutable union already.
            // Do not force a downgrade merely to traverse that intermediate.
            // Requeue the pinned alternative through the same admission checks,
            // retaining heap priority and the selected-count ceiling.
            if let Some(finer) = (targets[id].unwrap_or(next)..next).rev().find(|&i| {
                let range = leaf.lods[i].1;
                (range.count == 0 || available(range.file))
                    && !admissions.is_empty()
                    && admissions.iter().all(|a| a.pinned[range.file])
            }) {
                heap.push((score, std::cmp::Reverse(id), finer));
            }
            continue;
        }
        if let Some(reason) = reasons.as_mut().and_then(|r| r.get_mut(id)) {
            *reason = "selected target";
        }
        for (a, total) in admissions.iter().zip(&mut source_counts) {
            *total = next_cost(a, *total);
        }
        refs[old.file] -= 1;
        refs[new.file] += 1;
        count = new_count;
        chosen[id] = Some(next);
        if let Some(c) = candidate(id, next) {
            heap.push(c);
        }
    }
    if let Some((out, projection)) = trace.as_mut() {
        out.clear();
        for (id, (leaf, index)) in manifest
            .leaves
            .iter()
            .zip(&chosen)
            .take(MAX_LEAF_DIAGNOSTICS)
            .enumerate()
        {
            let culled = !intersects(leaf.bound, view.clip_from_local);
            let chosen_lod = index.and_then(|i| u32::try_from(leaf.lods[i].0).ok());
            let desired_lod = targets[id]
                .and_then(|index| leaf.lods.get(index))
                .and_then(|(level, _)| u32::try_from(*level).ok());
            let admission_reason = reasons
                .as_ref()
                .and_then(|r| r.get(id))
                .copied()
                .unwrap_or("selected target");
            let reason = if culled {
                "offscreen coarse coverage"
            } else if admission_reason != "selected target" {
                admission_reason
            } else if targets[id]
                .and_then(|index| leaf.lods.get(index))
                .is_some_and(|(_, r)| r.count > 0 && !available(r.file))
            {
                "unavailable/loading"
            } else {
                "selected target"
            };
            out.push(SogLeafDiagnostic {
                leaf: id,
                projected_bounds: diagnostics::project(leaf.bound, *projection),
                culled,
                desired_lod,
                chosen_lod,
                range: index.map(|i| leaf.lods[i].1),
                reason,
            });
        }
    }
    manifest
        .leaves
        .iter()
        .zip(chosen)
        .map(|(leaf, i)| i.map(|i| leaf.lods[i].1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_budget_resolves_equal_priorities_in_manifest_order() {
        let chosen = select(&scene(), Vec3::ZERO, 10, 100, |_| true);
        assert_eq!(
            chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [1, 0],
            "deployed distance-mode FIFO buys the first tied upgrade that fits"
        );
    }

    #[test]
    fn target_budget_stops_before_an_unaffordable_near_band() {
        let mut m = scene();
        m.file_counts[1] = 80;
        m.leaves[0].lods[0].1.count = 80;
        let chosen = select(&m, Vec3::new(-9., 0., 0.), 12, 100, |_| true);
        assert_eq!(
            chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0],
            "do not refine a distant region just because its upgrade is cheaper"
        );
    }

    #[test]
    fn target_budget_does_not_refine_elsewhere_while_near_target_downloads() {
        let m = scene();
        let chosen = select(&m, Vec3::new(-9., 0., 0.), 10, 100, |file| file != 1);
        assert_eq!(
            chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );
    }

    #[test]
    fn target_budget_fov_scale_keeps_relative_bands_without_frustum_changes() {
        let m = scene();
        for fov_scale in [0.25, 1.0, 4.0] {
            let chosen = select(
                &m,
                View {
                    fov_scale,
                    ..Vec3::new(-9., 0., 0.).into()
                },
                10,
                100,
                |_| true,
            );
            assert_eq!(
                chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
                [1, 0]
            );
        }
    }

    fn uneven_bounds_scene() -> Manifest {
        let leaf = |min_x, max_x, offset| {
            serde_json::json!({
                "bound": {"min": [min_x,-1,-11], "max": [max_x,1,-9]},
                "lods": {"0": {"file": 1,"offset": offset * 2,"count": 2},
                         "1": {"file": 0,"offset": offset,"count": 1}}
            })
        };
        let bound = serde_json::json!({"min": [1,-1,-11],"max": [402,1,-9]});
        let manifest = serde_json::json!({
            "lodLevels": 2,"filenames": ["coarse/meta.json","fine/meta.json"],
            "tree": {"bound": bound,"children": [
                {"bound": bound,"children": [leaf(1.,201.,0),leaf(5.,7.,1)]},
                {"bound": bound,"children": [leaf(300.,302.,2),leaf(400.,402.,3)]}
            ]}
        });
        Manifest::parse(&serde_json::to_vec(&manifest).expect("fixture JSON"))
            .expect("valid fixture")
    }

    #[test]
    fn full_authored_bounds_determine_detail_even_for_long_sparse_leaves() {
        let scene = uneven_bounds_scene();
        // Deployed dfcc50f measures from x=1, not the shrunken x=75.25 used
        // by local a957ec2. Full-box distance intentionally prefers the long
        // leaf; this does not claim that its splats fill the entire box.
        let chosen = select(&scene, Vec3::ZERO, 5, 100, |_| true);
        assert_eq!(
            chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [1, 0, 0, 0]
        );
    }

    #[test]
    fn full_visibility_bounds_keep_a_long_leaf_touching_the_frustum() {
        let scene = uneven_bounds_scene();
        let view = View {
            clip_from_local: Some(Mat4::perspective_infinite_reverse_rh(
                20f32.to_radians(),
                1.,
                0.1,
            )),
            ..Vec3::ZERO.into()
        };
        let chosen = select(&scene, view, 8, 100, |_| true);
        assert!(
            chosen[0].is_some(),
            "the long leaf's real edge intersects the narrow camera view"
        );
        assert!(intersects(scene.leaves[0].bound, view.clip_from_local));
        for (leaf, range) in scene.leaves[1..].iter().zip(&chosen[1..]) {
            assert!(!intersects(leaf.bound, view.clip_from_local));
            assert_eq!(*range, Some(leaf.lods.last().unwrap().1));
        }
    }

    fn scene() -> Manifest {
        Manifest::parse(
            br#"{
            "lodLevels": 2, "filenames": ["coarse/meta.json", "left/meta.json", "right/meta.json"],
            "tree": {"bound":{"min":[-10,-1,-1],"max":[10,1,1]}, "children":[
                {"bound":{"min":[-10,-1,-1],"max":[-8,1,1]},"lods":{
                    "0":{"file":1,"offset":0,"count":8},"1":{"file":0,"offset":0,"count":2}}},
                {"bound":{"min":[8,-1,-1],"max":[10,1,1]},"lods":{
                    "0":{"file":2,"offset":0,"count":8},"1":{"file":0,"offset":2,"count":2}}}
            ]}}
        "#,
        )
        .unwrap()
    }

    #[test]
    fn immutable_old_sources_are_charged_once_across_a_transition() {
        let m = scene();
        for (capacity, expected) in [(20, [1, 2]), (19, [1, 0]), (12, [1, 0])] {
            let chosen = select_admitted(
                &m,
                Vec3::new(-9., 0., 0.).into(),
                100,
                &[SourceAdmission {
                    costs: &[4, 8, 8],
                    pinned: &[true, true, false],
                    capacity,
                }],
                |_| true,
                None,
            );
            assert_eq!(
                chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn admission_summary_reports_source_rejection_without_allocating_leaf_trace() {
        let mut summary = AdmissionSummary::default();
        let result = select_with_summary(
            &scene(),
            Vec3::ZERO.into(),
            100,
            &[SourceAdmission {
                costs: &[4, 8, 8],
                pinned: &[true, false, false],
                capacity: 12,
            }],
            |_| true,
            None,
            &mut summary,
        );
        assert_eq!(
            result
                .iter()
                .flatten()
                .map(|range| range.count)
                .sum::<usize>(),
            10
        );
        assert!(summary.source_limited);
        assert!(!summary.selection_limited);
    }

    #[test]
    fn rejected_large_upgrade_does_not_starve_smaller_independent_upgrade() {
        let mut m = scene();
        m.file_counts[1] = 80;
        m.leaves[0].lods[0].1.count = 80;
        let chosen = select_admitted(
            &m,
            Vec3::new(-9., 0., 0.).into(),
            100,
            &[SourceAdmission {
                costs: &[4, 80, 8],
                pinned: &[true, false, false],
                capacity: 12,
            }],
            |_| true,
            None,
        );
        assert_eq!(
            chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [0, 2]
        );
    }

    #[test]
    fn retains_pinned_fine_when_cached_intermediate_transition_exceeds_headroom() {
        let mut m = scene();
        m.leaves.truncate(1);
        m.leaves[0].lods = vec![
            (
                0,
                SplatRange {
                    file: 2,
                    offset: 0,
                    count: 8,
                },
            ),
            (
                1,
                SplatRange {
                    file: 1,
                    offset: 0,
                    count: 6,
                },
            ),
            (
                2,
                SplatRange {
                    file: 0,
                    offset: 0,
                    count: 4,
                },
            ),
        ];
        let mut trace = Vec::new();
        let chosen = select_admitted(
            &m,
            Vec3::ZERO.into(),
            100,
            &[SourceAdmission {
                costs: &[4, 8, 8],
                pinned: &[true, false, true],
                capacity: 12,
            }],
            |_| true,
            Some((&mut trace, None)),
        );
        assert_eq!(
            chosen[0],
            Some(SplatRange {
                file: 2,
                offset: 0,
                count: 8
            })
        );
        assert_eq!(trace[0].chosen_lod, Some(0));
        assert_eq!(trace[0].reason, "selected target");
    }

    #[test]
    fn admissible_intermediate_survives_when_finest_jump_does_not_fit() {
        let mut m = scene();
        m.leaves.truncate(1);
        m.leaves[0].lods = vec![
            (
                0,
                SplatRange {
                    file: 1,
                    offset: 0,
                    count: 80,
                },
            ),
            (
                1,
                SplatRange {
                    file: 2,
                    offset: 0,
                    count: 8,
                },
            ),
            (
                2,
                SplatRange {
                    file: 0,
                    offset: 0,
                    count: 2,
                },
            ),
        ];
        let mut trace = Vec::new();
        let chosen = select_admitted(
            &m,
            Vec3::ZERO.into(),
            100,
            &[SourceAdmission {
                costs: &[4, 80, 8],
                pinned: &[true, false, false],
                capacity: 12,
            }],
            |_| true,
            Some((&mut trace, None)),
        );
        assert_eq!(chosen[0].unwrap().file, 2);
        assert_eq!(trace[0].reason, "source transition");
        assert_eq!(trace[0].desired_lod, Some(0));
        assert_eq!(trace[0].chosen_lod, Some(1));
    }

    #[test]
    fn shared_old_file_and_padded_source_costs_are_deduplicated() {
        let mut m = scene();
        m.leaves[1].lods[0].1.file = 1;
        let chosen = select_admitted(
            &m,
            Vec3::ZERO.into(),
            100,
            &[SourceAdmission {
                costs: &[32, 32, 32],
                pinned: &[true, true, false],
                capacity: 64,
            }],
            |_| true,
            None,
        );
        assert_eq!(
            chosen.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [1, 1]
        );
        let denied = select_admitted(
            &scene(),
            Vec3::ZERO.into(),
            100,
            &[SourceAdmission {
                costs: &[32, 32, 32],
                pinned: &[true, false, false],
                capacity: 63,
            }],
            |_| true,
            None,
        );
        assert_eq!(
            denied.iter().flatten().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );
    }

    #[test]
    fn trace_is_bounded_and_behind_eye_projection_is_not_fabricated() {
        let mut m = scene();
        while m.leaves.len() < 300 {
            m.leaves.push(super::super::manifest::Leaf {
                bound: Bounds {
                    min: Vec3::new(-1., -1., 4.),
                    max: Vec3::new(1., 1., 5.),
                },
                lods: m.leaves[0].lods.clone(),
            });
        }
        let mut trace = Vec::new();
        let clip = Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1., 0.1);
        let selected = select_admitted(
            &m,
            Vec3::ZERO.into(),
            10000,
            &[],
            |_| true,
            Some((&mut trace, Some(clip))),
        );
        assert_eq!(trace.len(), 256);
        assert_eq!(selected.len(), 300);
        assert!(trace[2].projected_bounds.is_none());
        assert!(!trace[2].culled);
        assert_eq!(trace[2].chosen_lod, Some(0));
        assert_eq!(trace[2].desired_lod, Some(0));
        assert_eq!(trace[2].leaf, 2);
    }

    #[test]
    fn spends_detail_on_the_nearest_region_and_reallocates_when_camera_moves() {
        let m = scene();
        let left = select(&m, Vec3::new(-9., 0., 0.), 10, 20, |_| true);
        let right = select(&m, Vec3::new(9., 0., 0.), 10, 20, |_| true);
        assert_eq!(
            left.iter().map(|r| r.unwrap().file).collect::<Vec<_>>(),
            [1, 0]
        );
        assert_eq!(
            right.iter().map(|r| r.unwrap().file).collect::<Vec<_>>(),
            [0, 2]
        );
    }

    #[test]
    fn retains_exact_coarse_ranges_until_replacement_file_is_available() {
        let m = scene();
        let selected = select(&m, Vec3::ZERO, 20, 20, |file| file == 0);
        assert_eq!(
            selected,
            vec![
                Some(SplatRange {
                    file: 0,
                    offset: 0,
                    count: 2
                }),
                Some(SplatRange {
                    file: 0,
                    offset: 2,
                    count: 2
                }),
            ]
        );
    }

    #[test]
    fn source_budget_counts_whole_files_and_pins_coarse_coverage() {
        let m = scene();
        // One coarse file (4) plus one finer file (8) fits, both finer files don't.
        let selected = select(&m, Vec3::new(-9., 0., 0.), 100, 12, |_| true);
        assert_eq!(
            selected.iter().flatten().map(|r| r.count).sum::<usize>(),
            10
        );
        assert_eq!(selected[1].unwrap().file, 0);
    }

    #[test]
    fn turning_camera_spends_budget_in_front() {
        let m = scene();
        let selected = select(
            &m,
            View {
                eye: Vec3::ZERO,
                forward: Vec3::X,
                fov_scale: 1.0,
                clip_from_local: None,
                ..Vec3::ZERO.into()
            },
            10,
            20,
            |_| true,
        );
        assert_eq!(selected[0].unwrap().file, 0);
        assert_eq!(selected[1].unwrap().file, 2);
    }

    #[test]
    fn coverage_survives_budget_below_coarsest_size() {
        let m = scene();
        let selected = select(&m, Vec3::ZERO, 1, 4, |_| true);
        assert_eq!(selected.iter().flatten().map(|r| r.count).sum::<usize>(), 4);
    }

    fn projected_view(clip_from_local: Mat4) -> View<'static> {
        View {
            clip_from_local: Some(clip_from_local),
            ..Vec3::ZERO.into()
        }
    }

    #[test]
    fn offscreen_leaf_reserves_only_coarse_coverage_before_visible_refinement() {
        let m = scene();
        // An orthographic box centered on the left leaf: x [-10,-8], y [-2,2].
        let clip = Mat4::orthographic_rh(-10., -8., -2., 2., -2., 2.);
        let selected = select(&m, projected_view(clip), 10, 20, |_| true);
        assert_eq!(selected[0].unwrap().file, 1);
        assert_eq!(selected[1], Some(m.leaves[1].lods.last().unwrap().1));
        assert_eq!(
            selected.iter().flatten().map(|r| r.count).sum::<usize>(),
            10
        );
        let tight = select(&m, projected_view(clip), 8, 20, |_| true);
        assert!(
            tight.iter().flatten().all(|r| r.file == 0),
            "fallback counts against the same budget"
        );
        // Visibility must not waive whole-file/coarse-source admission.
        let constrained = select(&m, projected_view(clip), 10, 11, |_| true);
        assert_eq!(constrained[0].unwrap().file, 0);
    }

    #[test]
    fn fully_offscreen_selection_retains_only_coarsest_available_coverage() {
        let m = scene();
        let selected = select(
            &m,
            projected_view(Mat4::orthographic_rh(-2., 2., -2., 2., -2., 2.)),
            100,
            100,
            |_| true,
        );
        assert_eq!(
            selected,
            m.leaves
                .iter()
                .map(|leaf| Some(leaf.lods.last().unwrap().1))
                .collect::<Vec<_>>()
        );
        let mut summary = AdmissionSummary::default();
        select_with_summary(
            &m,
            projected_view(Mat4::orthographic_rh(-2., 2., -2., 2., -2., 2.)),
            100,
            &[],
            |_| true,
            None,
            &mut summary,
        );
        assert!(
            !summary.selection_limited,
            "intentional offscreen coarsening is not a budget failure"
        );
    }

    #[test]
    fn visible_gaussian_support_keeps_coarse_then_fine_ranges() {
        // Same positions, anisotropic scales (0.4, 0.3, 0.2) and 45-degree
        // Z rotation as the production GPU footprint regression in the vendor.
        // These explicit 3-sigma support bounds have half extents
        // (3*sqrt((0.4²+0.3²)/2), same, 3*0.2). Do not use centre visibility
        // as the oracle: the GPU test observes pixels for all six visible cases.
        let half = Vec3::new(1.060_660_2, 1.060_660_2, 0.6);
        let clip = Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1., 0.1);
        for (position, visible) in [
            (Vec3::new(0., 0., -2.), true),
            (Vec3::new(2.4, 0., -2.), true),
            (Vec3::new(-2.4, 0., -2.), true),
            (Vec3::new(0., 2.4, -2.), true),
            (Vec3::new(0., -2.4, -2.), true),
            (Vec3::new(0., 0., -0.05), true),
            (Vec3::new(0., 0., 2.), false),
            (Vec3::new(20., 0., -2.), false),
        ] {
            let mut manifest = scene();
            manifest.leaves.truncate(1);
            manifest.leaves[0].bound = Bounds {
                min: position - half,
                max: position + half,
            };
            for (fine_ready, expected_range) in [
                (
                    false,
                    SplatRange {
                        file: 0,
                        offset: 0,
                        count: 2,
                    },
                ),
                (
                    true,
                    SplatRange {
                        file: 1,
                        offset: 0,
                        count: 8,
                    },
                ),
            ] {
                let selected = select(&manifest, projected_view(clip), 8, 20, |file| {
                    file == 0 || fine_ready
                });
                assert_eq!(
                    selected,
                    [Some(if visible {
                        expected_range
                    } else {
                        manifest.leaves[0].lods.last().unwrap().1
                    })],
                    "support at {position}, fine_ready={fine_ready}"
                );
            }
        }
    }

    #[test]
    fn expanded_sphere_frustum_retains_edge_overlap_and_rejects_behind_or_outside_bounds() {
        let clip = Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1., 0.1);
        let cases = [
            (Vec3::new(-0.5, -0.5, -5.), Vec3::new(0.5, 0.5, -4.), true),
            (Vec3::new(4.35, -0.5, -4.01), Vec3::new(4.8, 0.5, -4.), true),
            // The reference sphere retains tails outside the tighter AABB.
            (Vec3::new(4.5, -0.5, -4.01), Vec3::new(5., 0.5, -4.), true),
            (Vec3::new(5.4, -0.5, -4.01), Vec3::new(6., 0.5, -4.), false),
            (Vec3::new(-0.5, -0.5, 4.), Vec3::new(0.5, 0.5, 5.), false),
            // Bounds crossing the eye plane must fail open.
            (Vec3::new(20., -0.5, -1.), Vec3::new(21., 0.5, 1.), true),
            // Entirely closer than the near plane.
            (
                Vec3::new(-0.01, -0.01, -0.05),
                Vec3::new(0.01, 0.01, -0.01),
                false,
            ),
        ];
        for (min, max, retained) in cases {
            let mut m = scene();
            m.leaves.truncate(1);
            m.leaves[0].bound = super::super::manifest::Bounds { min, max };
            let selected = select(&m, projected_view(clip), 100, 100, |_| true);
            assert_eq!(
                intersects(m.leaves[0].bound, Some(clip)),
                retained,
                "bound {min:?}..{max:?}"
            );
            let index = if retained {
                0
            } else {
                m.leaves[0].lods.len() - 1
            };
            assert_eq!(selected[0], Some(m.leaves[0].lods[index].1));
        }
    }

    #[test]
    fn reference_sphere_retains_audited_kitchen_leaf_tail() {
        // Actual kitchen leaf 21: the production raster has 1008 lit pixels
        // here despite its authored AABB being wholly outside the left plane.
        // PlayCanvas a957ec2's GSplatOctreeNode + Frustum classifies its sphere
        // as intersecting at this pose (radius 5.037739), not outside.
        let bound = Bounds {
            min: Vec3::new(13.06204, -3.051272, -34.85479),
            max: Vec3::new(21.88249, 1.414806, -32.91338),
        };
        let projection = Mat4::perspective_infinite_reverse_rh(60f32.to_radians(), 1., 0.1);
        assert!(intersects(
            bound,
            Some(projection * Mat4::from_translation(Vec3::new(-24.8, -0.4, 31.)))
        ));
        assert!(!intersects(
            bound,
            Some(projection * Mat4::from_translation(Vec3::new(-35., -0.4, 31.)))
        ));
    }

    #[test]
    fn transformed_bounds_handle_rotated_translated_mirrored_nonuniform_scene() {
        let m = scene();
        let root = Mat4::from_scale_rotation_translation(
            Vec3::new(-1., 0.5, 2.),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(0., -9., -6.),
        );
        let clip =
            Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1., 0.1) * root;
        let selected = select(&m, projected_view(clip), 10, 20, |_| true);
        assert_eq!(selected[0].unwrap().file, 1);
        assert_eq!(selected[1], Some(m.leaves[1].lods.last().unwrap().1));
    }

    #[test]
    fn invalid_or_singular_projection_and_bounds_fail_open() {
        let m = scene();
        let expected = [true, true];
        for clip in [Mat4::ZERO, Mat4::from_cols_array(&[f32::NAN; 16])] {
            let selected = select(&m, projected_view(clip), 100, 100, |_| true);
            assert_eq!(
                selected.iter().map(Option::is_some).collect::<Vec<_>>(),
                expected
            );
        }
        let mut invalid = scene();
        invalid.leaves[0].bound.min.x = f32::NAN;
        let selected = select(&invalid, projected_view(Mat4::IDENTITY), 100, 100, |_| true);
        assert!(selected[0].is_some());
    }
}
