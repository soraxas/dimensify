//! Rank unstarted requests without changing targets, admission or ownership.
//! Coverage/switch tiers follow the newer PlayCanvas scheduling approach;
//! distance remains our pinned deployed-reference full-bound metric.
use super::{SplatRange, distance, manifest::Manifest, selection::View};
use std::{cmp::Ordering, collections::HashSet};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    // Keep the complete coarse fallback and independent environment ahead of
    // detail. This also prevents repeated refinement from starving coverage.
    Coarse,
    Environment,
    MissingCoverage,
    Refinement,
}

#[derive(Clone, Copy)]
struct Priority {
    tier: Tier,
    distance: f64,
    first_request: usize,
}

impl Priority {
    fn compare(self, other: Self) -> Ordering {
        self.tier
            .cmp(&other.tier)
            .then_with(|| self.distance.total_cmp(&other.distance))
            .then_with(|| self.first_request.cmp(&other.first_request))
    }
}

/// Prioritize source requests from visible replacement intent.
pub(super) fn requests(
    manifest: &Manifest,
    desired: &[Option<SplatRange>],
    displayed: &[SplatRange],
    view: View,
    environment: Option<usize>,
) -> Vec<usize> {
    let displayed: HashSet<_> = displayed
        .iter()
        .filter(|range| range.count > 0)
        .map(|range| (range.file, range.offset, range.count))
        .collect();
    let mut priorities: Vec<Option<Priority>> =
        vec![None; manifest.filenames.len() + usize::from(environment.is_some())];
    let mut sequence = 0;
    let mut offer = |file: usize, tier: Tier, distance: f64| {
        // Invalid camera math must not poison ordering or cause a sort panic.
        let distance = if distance.is_finite() {
            distance.max(0.)
        } else {
            f64::INFINITY
        };
        let first_request = priorities[file].map_or(sequence, |old| old.first_request);
        let next = Priority {
            tier,
            distance,
            first_request,
        };
        if priorities[file].is_none_or(|old| next.compare(old).is_lt()) {
            priorities[file] = Some(next);
        }
        sequence += 1;
    };
    for (index, leaf) in manifest.leaves.iter().enumerate() {
        let distance = distance::full_bounds_squared(leaf.bound, view.eye, view.forward)
            * f64::from(view.fov_scale).powi(2);
        if let Some((_, range)) = leaf.lods.last() {
            offer(range.file, Tier::Coarse, distance);
        }
        if let Some(range) = desired
            .get(index)
            .copied()
            .flatten()
            .filter(|range| range.count > 0)
        {
            let covered = leaf
                .lods
                .iter()
                .any(|(_, range)| displayed.contains(&(range.file, range.offset, range.count)));
            offer(
                range.file,
                if covered {
                    Tier::Refinement
                } else {
                    Tier::MissingCoverage
                },
                distance,
            );
        }
    }
    if let Some(file) = environment {
        offer(file, Tier::Environment, 0.);
    }
    let mut ranked: Vec<_> = priorities
        .into_iter()
        .enumerate()
        .filter_map(|(file, priority)| priority.map(|priority| (file, priority)))
        .collect();
    ranked.sort_unstable_by(|(_, a), (_, b)| a.compare(*b));
    ranked.into_iter().map(|(file, _)| file).collect()
}
