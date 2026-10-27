//! Bounded whole-file relocation. A final file set is planned separately from
//! the old/new transition union; the latter is released through coarse coverage.

use super::manifest::{Manifest, SplatRange};
use super::selection::{self, AdmissionSummary, SourceAdmission, View};

#[cfg(test)]
#[path = "tests/pressure_replay.rs"]
mod pressure_replay;

/// Check whether the proposed source union satisfies admission.
pub(super) fn fits(files: &[bool], admissions: &[SourceAdmission<'_>]) -> bool {
    admissions.iter().all(|a| {
        files
            .iter()
            .zip(a.costs)
            .filter(|(used, _)| **used)
            .fold(0usize, |sum, (_, cost)| sum.saturating_add(*cost))
            <= a.capacity
    })
}

/// Choose a covered source union under the current admission constraints.
pub(super) fn select_files(
    manifest: &Manifest,
    view: View,
    budget: usize,
    admissions: &[SourceAdmission<'_>],
    files: &[bool],
) -> Vec<Option<SplatRange>> {
    // Files in this final set are charged in full. Intermediate LODs in other
    // files are not dependencies: SOG chunks decode independently.
    let final_admissions: Vec<_> = admissions
        .iter()
        .map(|a| SourceAdmission {
            costs: a.costs,
            pinned: files,
            capacity: a.capacity,
        })
        .collect();
    selection::select_with_summary(
        manifest,
        view,
        budget,
        &final_admissions,
        |i| files[i],
        None,
        &mut AdmissionSummary::default(),
    )
}

/// Prepared once per planner invocation, never retained across camera, quality,
/// budget or manifest changes. A fitting final source mask charges all sources
/// up front, so independent leaves can choose their finest available range no
/// finer than their budgeted target. Their sum cannot exceed the target sum.
struct PreparedSelection {
    leaves: Vec<PreparedLeaf>,
    bands: [f64; 32],
}

#[derive(Default)]
struct PreparedLeaf {
    lods: Vec<(SplatRange, usize)>,
    distance: f64,
}

impl PreparedLeaf {
    fn chosen(&self, files: &[bool]) -> Option<(SplatRange, usize)> {
        self.lods
            .iter()
            .copied()
            .find(|(range, _)| range.count == 0 || files[range.file])
    }
}

impl PreparedSelection {
    fn new(manifest: &Manifest, view: View, budget: usize) -> Self {
        let targets = selection::target_ranges(manifest, view, budget);
        let leaves = manifest
            .leaves
            .iter()
            .zip(targets)
            .map(|(leaf, target)| {
                let Some(target) = target else {
                    return PreparedLeaf::default();
                };
                let index = leaf
                    .lods
                    .iter()
                    .position(|(_, range)| *range == target)
                    .unwrap();
                let distance =
                    super::distance::full_bounds_squared(leaf.bound, view.eye, view.forward)
                        .max(1e-12);
                PreparedLeaf {
                    lods: leaf.lods[index..]
                        .iter()
                        .map(|(level, range)| (*range, *level))
                        .collect(),
                    distance,
                }
            })
            .collect();
        Self {
            leaves,
            bands: std::array::from_fn(|lod| 9.0_f64.powi(lod as i32)),
        }
    }

    fn chosen<'a>(
        &'a self,
        files: &'a [bool],
    ) -> impl Iterator<Item = Option<(SplatRange, usize)>> + 'a {
        self.leaves.iter().map(move |leaf| leaf.chosen(files))
    }

    #[cfg(test)]
    fn score(&self, files: &[bool]) -> f64 {
        self.delta(files, &vec![false; files.len()]).0
    }

    /// Compare only changed bands before aggregation. An unchanged leaf around
    /// the eye can contribute ~1e16; adding a distant gain to that total loses
    /// precision. Subtracting cumulative per-leaf scores also loses fine-band
    /// gains for sparse hierarchies with large coarsest LOD indices.
    fn delta(&self, candidate: &[bool], previous: &[bool]) -> (f64, f64) {
        let mut total = 0.0;
        let mut magnitude = 0.0;
        for leaf in &self.leaves {
            let Some((_, coarse)) = leaf.lods.last() else {
                continue;
            };
            let next = leaf.chosen(candidate).map_or(*coarse, |(_, lod)| lod);
            let old = leaf.chosen(previous).map_or(*coarse, |(_, lod)| lod);
            if next == old {
                continue;
            }
            let change =
                self.bands[next.min(old)..next.max(old)].iter().sum::<f64>() / leaf.distance;
            total += if next < old { change } else { -change };
            magnitude += change;
        }
        (total, magnitude)
    }

    fn improves(&self, candidate: &[bool], previous: &[bool]) -> bool {
        let (gain, changed) = self.delta(candidate, previous);
        // Require gains to outweigh sacrificed detail by ~10.5% before
        // replacing competing whole files. Tiny view changes otherwise flip
        // equal-value sources repeatedly, paying decode/upload on every turn.
        // Only changed bands contribute: a pure upgrade has gain == changed
        // and is never suppressed by unrelated high-value near-eye detail.
        gain > (changed * 0.05).max(1e-9)
    }

    fn trim(&self, files: &[bool], fixed: &[bool]) -> Vec<bool> {
        let mut used = fixed.to_vec();
        self.trim_into(files, fixed, &mut used);
        used
    }

    fn trim_into(&self, files: &[bool], fixed: &[bool], used: &mut [bool]) {
        used.copy_from_slice(fixed);
        for (range, _) in self.chosen(files).flatten().filter(|(r, _)| r.count > 0) {
            used[range.file] = true;
        }
    }
}

// A bounded escape from the one-incoming local minimum. No quadratic search
// across the entire manifest: at most 12 shortlisted absent files / 66 pairs,
// and at most 128 extra leaf-selection scans including retained-source checks.
const PAIR_FILES: usize = 12;
const PAIR_EVALUATIONS: usize = 128;

fn retain_preferred(
    candidate: &mut [bool],
    fixed: &[bool],
    keep: &[(usize, f64)],
    admissions: &[SourceAdmission<'_>],
    prepared: &PreparedSelection,
    evaluations_left: &mut usize,
) {
    let mut used = fixed.to_vec();
    for &(file, _) in keep {
        if candidate[file] {
            continue;
        }
        if *evaluations_left == 0 {
            break;
        }
        candidate[file] = true;
        *evaluations_left -= 1;
        // Admission is for the final destination, not the temporary union.
        // A retained fine source can eliminate the last use of a middle source
        // already added earlier in this loop. Charging both until the end can
        // reject a fitting improvement and retire unrelated useful detail.
        // Fixed coverage remains charged; live transition ownership is checked
        // separately by the streaming lifecycle before acquiring/releasing it.
        prepared.trim_into(candidate, fixed, &mut used);
        if !used[file] || !fits(&used, admissions) {
            candidate[file] = false;
        } else {
            candidate.copy_from_slice(&used);
        }
    }
}

/// Whole-file replacement with one target allocation per invocation. Preserve
/// the one-incoming candidates, then try bounded pairs of absent sources. Hard
/// CPU/GPU caps and covered handoff are unchanged; changed-band hysteresis
/// preserves the preferred set on near ties without delaying pure upgrades.
pub(super) fn plan_files(
    manifest: &Manifest,
    view: View,
    budget: usize,
    admissions: &[SourceAdmission<'_>],
    fixed: &[bool],
    preferred: &[bool],
    available: impl Fn(usize) -> bool,
) -> Vec<bool> {
    if !fits(fixed, admissions) {
        return fixed.to_vec();
    }
    let available: Vec<_> = (0..manifest.filenames.len()).map(available).collect();
    let prepared = PreparedSelection::new(manifest, view, budget);
    let mut best: Vec<_> = fixed
        .iter()
        .zip(preferred)
        .enumerate()
        .map(|(i, (fixed, preferred))| *fixed || (*preferred && available[i]))
        .collect();
    if !fits(&best, admissions) {
        best.clone_from_slice(fixed);
    }
    let mut keep = Vec::new();
    let mut incoming = Vec::new();
    let remaining: Vec<_> = admissions
        .iter()
        .map(|a| {
            a.capacity.saturating_sub(
                fixed
                    .iter()
                    .zip(a.costs)
                    .filter(|(used, _)| **used)
                    .fold(0usize, |sum, (_, cost)| sum.saturating_add(*cost)),
            )
        })
        .collect();
    for file in 0..available.len() {
        if fixed[file] || !available[file] {
            continue;
        }
        let mut mask = fixed.to_vec();
        mask[file] = true;
        if !fits(&mask, admissions) {
            continue;
        }
        let score = prepared.delta(&mask, fixed).0;
        if preferred[file] {
            keep.push((file, score));
        } else if score > 1e-9 {
            // Normalize each independent admission dimension; do not add CPU
            // bytes to GPU rows. Shared/fixed sources are charged only once.
            let cost = admissions
                .iter()
                .zip(&remaining)
                .map(|(a, remaining)| a.costs[file] as f64 / (*remaining).max(1) as f64)
                .fold(0.0, f64::max)
                .max(f64::EPSILON);
            incoming.push((file, score / cost));
        }
    }
    keep.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    for file in 0..available.len() {
        if fixed[file] || !available[file] {
            continue;
        }
        let mut candidate = fixed.to_vec();
        candidate[file] = true;
        if !fits(&candidate, admissions) {
            continue;
        }
        let mut retention_scans = keep.len();
        retain_preferred(
            &mut candidate,
            fixed,
            &keep,
            admissions,
            &prepared,
            &mut retention_scans,
        );
        if prepared.improves(&candidate, &best) {
            best = candidate;
        }
    }
    incoming.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    incoming.truncate(PAIR_FILES);
    let mut evaluations_left = PAIR_EVALUATIONS;
    'pairs: for (index, &(a, _)) in incoming.iter().enumerate() {
        for &(b, _) in &incoming[index + 1..] {
            if evaluations_left == 0 {
                break 'pairs;
            }
            let mut candidate = fixed.to_vec();
            candidate[a] = true;
            candidate[b] = true;
            if !fits(&candidate, admissions) {
                continue;
            }
            // Reserve the final score scan before spending work on retention.
            evaluations_left -= 1;
            retain_preferred(
                &mut candidate,
                fixed,
                &keep,
                admissions,
                &prepared,
                &mut evaluations_left,
            );
            if prepared.improves(&candidate, &best) {
                best = candidate;
            }
        }
    }
    prepared.trim(&best, fixed)
}

/// Only release sources here. Acquisition follows renderer readiness; a CPU
/// resident file is not necessarily present in the GPU source cache.
pub(super) fn covered_bridge(
    manifest: &Manifest,
    target: &[Option<SplatRange>],
    files: &[bool],
    admissions: &[SourceAdmission<'_>],
    resident: impl Fn(usize) -> bool,
) -> Option<Vec<SplatRange>> {
    let mut bridge = Vec::new();
    for (leaf, target) in manifest.leaves.iter().zip(target) {
        let Some(target) = target else { continue };
        let index = leaf.lods.iter().position(|(_, r)| r == target)?;
        let (_, range) = leaf.lods[index..].iter().find(|(_, range)| {
            range.count == 0
                || (files[range.file]
                    && resident(range.file)
                    && admissions.iter().all(|a| a.pinned[range.file]))
        })?;
        if range.count > 0 {
            bridge.push(*range);
        }
    }
    Some(bridge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    #[ignore = "native policy timing fixture; run explicitly with --nocapture"]
    fn kitchen_planning_cost_fixture() {
        let manifest = Manifest::parse(include_bytes!("fixtures/sanatorium-lod.json")).unwrap();
        let costs: Vec<_> = manifest
            .file_counts
            .iter()
            .map(|n| super::super::cache::reserved_rows(*n))
            .collect();
        let mut fixed = vec![false; costs.len()];
        for file in manifest.coarse_files() {
            fixed[file] = true;
        }
        let admissions = [SourceAdmission {
            costs: &costs,
            pinned: &fixed,
            capacity: 256 * 1024 * 1024 / 64,
        }];
        let view = Vec3::new(-2.01, 0.127, -6.347).into();
        let preferred = plan_files(
            &manifest,
            view,
            3_313_696,
            &admissions,
            &fixed,
            &fixed,
            |_| true,
        );
        let start = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(plan_files(
                &manifest,
                view,
                3_313_696,
                &admissions,
                &fixed,
                &preferred,
                |_| true,
            ));
        }
        eprintln!(
            "kitchen planning: 100 calls, {:.3} ms total; 101 leaves / 46 files; native optimized test profile, no GPU",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }

    fn pair_manifest() -> Manifest {
        Manifest::parse(
            br#"{
            "lodLevels":3,
            "filenames":["coarse/meta.json","middle/meta.json","left/meta.json","right/meta.json"],
            "tree":{"bound":{"min":[-1,-1,-2],"max":[1,1,-1]},"children":[
                {"bound":{"min":[-1,-1,-2],"max":[0,1,-1]},"lods":{
                    "0":{"file":2,"offset":0,"count":5},
                    "1":{"file":1,"offset":0,"count":4},
                    "2":{"file":0,"offset":0,"count":1}}},
                {"bound":{"min":[0,-1,-2],"max":[1,1,-1]},"lods":{
                    "0":{"file":3,"offset":0,"count":5},
                    "1":{"file":1,"offset":4,"count":4},
                    "2":{"file":0,"offset":1,"count":1}}}
            ]}
        }"#,
        )
        .expect("valid independent source fixture")
    }

    #[test]
    fn small_eye_oscillation_keeps_sources_but_substantial_movement_relocates() {
        let mut manifest = pair_manifest();
        manifest.leaves[0].bound.min.x = -2.;
        manifest.leaves[0].bound.max.x = -1.;
        manifest.leaves[1].bound.min.x = 1.;
        manifest.leaves[1].bound.max.x = 2.;
        let fixed = [true, false, false, false];
        let left = [true, false, true, false];
        let right = [true, false, false, true];
        let admissions = [SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &fixed,
            capacity: 7, // Coarse coverage plus exactly one fine source.
        }];
        let plan = |x, preferred: &[bool]| {
            plan_files(
                &manifest,
                Vec3::new(x, 0., 0.).into(),
                10,
                &admissions,
                &fixed,
                preferred,
                |file| file != 1,
            )
        };
        let mut retained = left.to_vec();
        for x in [0.01, -0.01, 0.02, -0.02, 0.01] {
            retained = plan(x, &retained);
            assert_eq!(retained, left, "minor movement must not churn a full cache");
            assert!(fits(&retained, &admissions));
        }
        retained = plan(0.8, &retained);
        assert_eq!(
            retained, right,
            "nearby detail must still displace distant detail"
        );
        assert_eq!(
            plan(-0.8, &retained),
            left,
            "return travel must also refine"
        );
    }

    #[test]
    fn retention_admits_a_fine_source_that_supersedes_the_last_use_of_a_middle_source() {
        let manifest = pair_manifest();
        let fixed = [true, false, false, false];
        let mut candidate = [true, false, true, false];
        let admissions = [SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &fixed,
            capacity: 15,
        }];
        let prepared = PreparedSelection::new(&manifest, Vec3::ZERO.into(), 10);
        // The middle source fits first and still supplies the right leaf. Once
        // the right fine source is retained, neither leaf uses the middle file.
        let mut scans = 2;
        retain_preferred(
            &mut candidate,
            &fixed,
            &[(1, 18.0), (3, 10.0)],
            &admissions,
            &prepared,
            &mut scans,
        );
        assert_eq!(candidate, [true, false, true, true]);
        assert!(fits(&candidate, &admissions));
    }

    #[test]
    fn retention_pruning_preserves_fixed_sources_independent_caps_and_work_bounds() {
        let manifest = pair_manifest();
        let prepared = PreparedSelection::new(&manifest, Vec3::ZERO.into(), 10);
        for (pin_middle, gpu_capacity, scans, expected) in [
            (false, 15, 0, [true, false, true, false]),
            (false, 15, 1, [true, true, true, false]),
            (false, 15, 2, [true, false, true, true]),
            (true, 15, 2, [true, true, true, false]),
            (false, 11, 2, [true, false, true, false]),
        ] {
            let fixed = [true, pin_middle, false, false];
            let mut candidate = [true, pin_middle, true, false];
            let admissions = [
                SourceAdmission {
                    costs: &manifest.file_counts,
                    pinned: &fixed,
                    capacity: 20,
                },
                SourceAdmission {
                    costs: &manifest.file_counts,
                    pinned: &fixed,
                    capacity: gpu_capacity,
                },
            ];
            let mut remaining = scans;
            retain_preferred(
                &mut candidate,
                &fixed,
                &[(1, 18.0), (3, 10.0)],
                &admissions,
                &prepared,
                &mut remaining,
            );
            assert_eq!(candidate, expected);
            assert!(fits(&candidate, &admissions));
            assert!(remaining <= scans);
        }
    }

    #[test]
    fn two_independent_fine_sources_can_replace_one_shared_intermediate() {
        let manifest = pair_manifest();
        let fixed = [true, false, false, false];
        let preferred = [true, true, false, false];
        let admissions = [SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &fixed,
            capacity: 12,
        }];
        let view = Vec3::ZERO.into();
        let planned = plan_files(&manifest, view, 10, &admissions, &fixed, &preferred, |_| {
            true
        });
        let pair = [true, false, true, true];
        assert!(fits(&pair, &admissions));
        let score = |files: &[bool]| PreparedSelection::new(&manifest, view, 10).score(files);
        assert_eq!(score(&preferred), 18.0);
        assert_eq!(score(&[true, false, true, false]), 10.0);
        assert_eq!(score(&[true, false, false, true]), 10.0);
        assert_eq!(score(&pair), 20.0);
        assert_eq!(planned, pair);
        let target = select_files(&manifest, view, 10, &admissions, &planned);
        assert_eq!(
            covered_bridge(&manifest, &target, &planned, &admissions, |file| file <= 1),
            Some(
                manifest
                    .leaves
                    .iter()
                    .map(|leaf| leaf.lods.last().unwrap().1)
                    .collect()
            ),
            "the superior pair must still transition through pinned coarse coverage"
        );
    }

    #[test]
    fn unchanged_near_eye_detail_does_not_hide_a_fitting_distant_upgrade() {
        let manifest = Manifest::parse(
            br#"{
            "lodLevels":6,
            "filenames":["coarse/meta.json","near/meta.json","far/meta.json"],
            "tree":{"bound":{"min":[-1,-1,-101],"max":[1,1,1]},"children":[
                {"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{
                    "0":{"file":1,"offset":0,"count":2},
                    "5":{"file":0,"offset":0,"count":1}}},
                {"bound":{"min":[-1,-1,-101],"max":[1,1,-100]},"lods":{
                    "0":{"file":2,"offset":0,"count":2},
                    "5":{"file":0,"offset":1,"count":1}}}
            ]}
        }"#,
        )
        .unwrap();
        let fixed = [true, false, false];
        let preferred = [true, true, false];
        let expected = [true, true, true];
        let admissions = [SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &fixed,
            capacity: 6,
        }];
        assert!(fits(&expected, &admissions));
        let actual = plan_files(
            &manifest,
            Vec3::ZERO.into(),
            4,
            &admissions,
            &fixed,
            &preferred,
            |_| true,
        );
        assert_eq!(
            actual, expected,
            "unchanged near-eye score must not swallow a positive distant gain"
        );
    }

    #[test]
    fn finest_upgrade_is_not_lost_against_large_shared_coarse_band_benefit() {
        let manifest = Manifest::parse(
            br#"{
            "lodLevels":32,
            "filenames":["coarse/meta.json","middle/meta.json","fine/meta.json"],
            "tree":{"bound":{"min":[-1,-1,-2],"max":[1,1,-1]},"lods":{
                "0":{"file":2,"offset":0,"count":3},
                "1":{"file":1,"offset":0,"count":2},
                "31":{"file":0,"offset":0,"count":1}}}
        }"#,
        )
        .unwrap();
        let fixed = [true, false, false];
        let preferred = [true, true, false];
        let admissions = [SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &fixed,
            capacity: 6,
        }];
        assert_eq!(
            plan_files(
                &manifest,
                Vec3::ZERO.into(),
                3,
                &admissions,
                &fixed,
                &preferred,
                |_| true
            ),
            [true, false, true],
            "compare the changed LOD band instead of subtracting cumulative benefits"
        );
    }

    #[test]
    fn captured_floor_preserves_near_eye_sources_and_stays_within_admission() {
        #[derive(serde::Deserialize)]
        struct Capture {
            eye: Vec3,
            forward: Vec3,
            selection_budget: usize,
            cache_rows: usize,
            displayed_files: Vec<usize>,
            desired_lods: Vec<Option<usize>>,
        }
        let capture: Capture =
            serde_json::from_str(include_str!("fixtures/floor-relocation.json")).unwrap();
        let mut manifest = Manifest::parse(include_bytes!("fixtures/sanatorium-lod.json")).unwrap();
        assert_eq!(capture.desired_lods.len(), manifest.leaves.len());
        let mut quality = Vec::new();
        manifest.leaves = std::mem::take(&mut manifest.leaves)
            .into_iter()
            .zip(capture.desired_lods)
            .filter_map(|(leaf, desired)| {
                let desired = desired?;
                quality.push(
                    leaf.lods
                        .iter()
                        .position(|(lod, _)| *lod == desired)
                        .unwrap(),
                );
                Some(leaf)
            })
            .collect();
        let costs: Vec<_> = manifest
            .file_counts
            .iter()
            .map(|count| super::super::cache::reserved_rows(*count))
            .collect();
        let mut fixed = vec![false; costs.len()];
        for file in manifest.coarse_files() {
            fixed[file] = true;
        }
        let mut preferred = fixed.clone();
        for file in capture.displayed_files {
            preferred[file] = true;
        }
        let admissions = [SourceAdmission {
            costs: &costs,
            pinned: &fixed,
            capacity: capture.cache_rows,
        }];
        let view = View {
            eye: capture.eye,
            forward: capture.forward,
            quality: Some(&quality),
            ..capture.eye.into()
        };
        let prepared = PreparedSelection::new(&manifest, view, capture.selection_budget);
        let planned = plan_files(
            &manifest,
            view,
            capture.selection_budget,
            &admissions,
            &fixed,
            &preferred,
            |_| true,
        );
        let old = select_files(
            &manifest,
            view,
            capture.selection_budget,
            &admissions,
            &preferred,
        );
        let new = select_files(
            &manifest,
            view,
            capture.selection_budget,
            &admissions,
            &planned,
        );
        eprintln!(
            "floor relocation: files {:?} -> {:?}, splats {} -> {}, changed benefit {:?}",
            preferred
                .iter()
                .enumerate()
                .filter_map(|(i, used)| used.then_some(i))
                .collect::<Vec<_>>(),
            planned
                .iter()
                .enumerate()
                .filter_map(|(i, used)| used.then_some(i))
                .collect::<Vec<_>>(),
            old.iter().flatten().map(|r| r.count).sum::<usize>(),
            new.iter().flatten().map(|r| r.count).sum::<usize>(),
            prepared.delta(&planned, &preferred)
        );
        assert!(fits(&planned, &admissions));
        assert!(
            planned[24] && planned[27],
            "preserve the two displayed near-eye LOD0 sources"
        );
        assert!(prepared.delta(&planned, &preferred).0 >= 0.0);
        assert!(new.iter().flatten().map(|r| r.count).sum::<usize>() <= capture.selection_budget);
        assert_eq!(
            plan_files(
                &manifest,
                view,
                capture.selection_budget,
                &admissions,
                &fixed,
                &planned,
                |_| true
            ),
            planned,
            "an unchanged camera must not oscillate between equivalent source sets"
        );
    }

    #[test]
    fn prepared_choices_match_admission_selection_for_sparse_zero_and_budgeted_ranges() {
        for sparse_zero in [false, true] {
            let mut manifest = pair_manifest();
            if sparse_zero {
                // Valid sparse levels, with a zero-sized coarsest range and
                // contiguous shared coarse-file offsets.
                for leaf in &mut manifest.leaves {
                    leaf.lods[1].0 = 2;
                    leaf.lods[2].0 = 5;
                }
                manifest.leaves[0].lods[2].1.count = 0;
                manifest.leaves[1].lods[2].1.offset = 0;
                manifest.file_counts[0] = 1;
            }
            let fixed = [true, false, false, false];
            for budget in [0, 2, 5, 9, 10, 100] {
                for capacity in [2, 7, 10, 12, 20] {
                    let admissions = [SourceAdmission {
                        costs: &manifest.file_counts,
                        pinned: &fixed,
                        capacity,
                    }];
                    for quality in [None, Some([1, 0])] {
                        for clip in [
                            None,
                            Some(Mat4::orthographic_rh(-3., -2., -1., 1., 0.1, 10.)),
                        ] {
                            let view = View {
                                quality: quality.as_ref().map(|q| q.as_slice()),
                                clip_from_local: clip,
                                ..Vec3::ZERO.into()
                            };
                            let prepared = PreparedSelection::new(&manifest, view, budget);
                            for bits in 0..8 {
                                let files = [true, bits & 1 != 0, bits & 2 != 0, bits & 4 != 0];
                                if !fits(&files, &admissions) {
                                    continue;
                                }
                                let expected =
                                    select_files(&manifest, view, budget, &admissions, &files);
                                let actual: Vec<_> = prepared
                                    .chosen(&files)
                                    .map(|choice| choice.map(|(r, _)| r))
                                    .collect();
                                assert_eq!(
                                    actual, expected,
                                    "sparse={sparse_zero}, budget={budget}, capacity={capacity}, mask={bits}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pair_swaps_respect_both_admission_domains_failures_and_quality_targets() {
        let manifest = pair_manifest();
        let fixed = [true, false, false, false];
        let preferred = [true, true, false, false];
        for (second_capacity, available_right, quality, expected) in [
            (20, true, None, [true, false, true, true]),
            (8, true, None, preferred),
            (20, false, None, preferred),
            (20, true, Some([1, 1]), preferred),
        ] {
            let admissions = [
                SourceAdmission {
                    costs: &manifest.file_counts,
                    pinned: &fixed,
                    capacity: 12,
                },
                SourceAdmission {
                    costs: &[2, 4, 5, 5],
                    pinned: &fixed,
                    capacity: second_capacity,
                },
            ];
            let view = View {
                quality: quality.as_ref().map(|q| q.as_slice()),
                ..Vec3::ZERO.into()
            };
            let actual = plan_files(
                &manifest,
                view,
                10,
                &admissions,
                &fixed,
                &preferred,
                |file| file != 3 || available_right,
            );
            assert_eq!(actual, expected);
            assert!(fits(&actual, &admissions));
            for _ in 0..10 {
                assert_eq!(
                    plan_files(
                        &manifest,
                        view,
                        10,
                        &admissions,
                        &fixed,
                        &actual,
                        |file| file != 3 || available_right
                    ),
                    actual,
                    "stable camera must not churn sources"
                );
            }
        }
    }

    #[test]
    fn prepared_kitchen_choices_match_existing_selector_with_fitting_source_masks() {
        let manifest = Manifest::parse(include_bytes!("fixtures/sanatorium-lod.json")).unwrap();
        let mut fixed = vec![false; manifest.filenames.len()];
        for file in manifest.coarse_files() {
            fixed[file] = true;
        }
        let admissions = [SourceAdmission {
            costs: &manifest.file_counts,
            pinned: &fixed,
            capacity: 4_194_304,
        }];
        for eye in [Vec3::new(-2.01, 0.127, -6.347), Vec3::new(8., 1., -35.)] {
            for budget in [1_000_000, 3_313_696] {
                let prepared = PreparedSelection::new(&manifest, eye.into(), budget);
                for a in 0..manifest.filenames.len() {
                    for b in a..manifest.filenames.len() {
                        let mut files = fixed.clone();
                        files[a] = true;
                        files[b] = true;
                        if !fits(&files, &admissions) {
                            continue;
                        }
                        let actual: Vec<_> = prepared
                            .chosen(&files)
                            .map(|choice| choice.map(|(r, _)| r))
                            .collect();
                        assert_eq!(
                            actual,
                            select_files(&manifest, eye.into(), budget, &admissions, &files)
                        );
                    }
                }
            }
        }
    }
}
