//! Per-leaf replacement intent, independent of camera evaluation and transport.
//! Renderer generations, not this ledger, own the actual source allocations.
use super::{SplatRange, manifest::Manifest};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
pub(super) struct Transition {
    pub old: Option<SplatRange>,
    pub target: Option<SplatRange>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(file: usize, offset: usize, count: usize) -> SplatRange {
        SplatRange {
            file,
            offset,
            count,
        }
    }

    #[test]
    fn ready_downgrades_release_count_before_independent_upgrades() {
        let replacements = Replacements {
            leaves: vec![
                Transition {
                    old: Some(range(0, 0, 10)),
                    target: Some(range(1, 0, 100)),
                },
                Transition {
                    old: Some(range(2, 0, 100)),
                    target: Some(range(3, 0, 10)),
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            replacements.resolve(110, |_| true),
            [range(1, 0, 100), range(3, 0, 10)]
        );
        assert_eq!(
            replacements.resolve(110, |file| file != 3),
            [range(0, 0, 10), range(2, 0, 100)],
            "unready downgrade cannot fund an upgrade"
        );
    }

    #[test]
    fn shared_file_completion_preserves_distinct_leaf_intervals() {
        let replacements = Replacements {
            leaves: vec![
                Transition {
                    old: Some(range(0, 0, 1)),
                    target: Some(range(1, 0, 8)),
                },
                Transition {
                    old: Some(range(0, 1, 1)),
                    target: Some(range(1, 8, 8)),
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            replacements.resolve(16, |_| true),
            [range(1, 0, 8), range(1, 8, 8)]
        );
        assert_eq!(
            replacements.resolve(16, |file| file == 0),
            [range(0, 0, 1), range(0, 1, 1)]
        );
    }
}

#[derive(Default)]
pub(super) struct Replacements {
    pub leaves: Vec<Transition>,
    /// Policy-ranked, deduplicated file requests. In-flight requests are owned
    /// separately by Entry, so superseding this list never cancels reservations.
    pub requests: Vec<usize>,
    pub dirty: bool,
}

impl Replacements {
    /// Preserve current owners before spending count on independent upgrades.
    /// Resident selection supplies coverage for leaves without an old owner.
    /// A reduced cap can require its coarser fallback instead of retaining old
    /// detail: continuity must never become permission to exceed a hard cap.
    pub fn policy_selection(
        manifest: &Manifest,
        targets: &[Option<SplatRange>],
        current: &[SplatRange],
        fallback: Vec<SplatRange>,
        budget: usize,
        mut ready: impl FnMut(usize) -> bool,
    ) -> Vec<SplatRange> {
        let mut transitions = Self::default();
        transitions.reconcile(manifest, targets, current);
        let fallback_ranges: HashSet<_> = fallback
            .iter()
            .map(|range| (range.file, range.offset, range.count))
            .collect();
        for (leaf, transition) in manifest.leaves.iter().zip(&mut transitions.leaves) {
            if transition.old.is_none_or(|range| !ready(range.file)) {
                transition.old = leaf.lods.iter().map(|(_, range)| *range).find(|range| {
                    fallback_ranges.contains(&(range.file, range.offset, range.count))
                });
            }
        }
        let resolved = transitions.resolve(budget, ready);
        if resolved
            .iter()
            .fold(0usize, |sum, range| sum.saturating_add(range.count))
            <= budget
        {
            resolved
        } else {
            fallback
        }
    }

    /// Reconcile replacement intents with current source ownership.
    pub fn reconcile(
        &mut self,
        manifest: &Manifest,
        targets: &[Option<SplatRange>],
        current: &[SplatRange],
    ) {
        let current: HashSet<_> = current
            .iter()
            .map(|range| (range.file, range.offset, range.count))
            .collect();
        self.leaves = manifest
            .leaves
            .iter()
            .zip(targets)
            .map(|(leaf, target)| Transition {
                old: leaf
                    .lods
                    .iter()
                    .map(|(_, range)| *range)
                    .find(|range| current.contains(&(range.file, range.offset, range.count))),
                target: *target,
            })
            .collect();
        self.dirty = false;
    }

    /// Report whether a replacement still requires this source.
    pub fn wants(&self, file: usize) -> bool {
        self.leaves
            .iter()
            .any(|leaf| leaf.target.is_some_and(|range| range.file == file))
    }

    /// Resolve only the accepted targets. Apply ready downgrades before upgrades
    /// so a mixed old/new selection respects the count cap as well as the final
    /// target. Missing targets retain their old representation, never a hole.
    /// Source union admission and renderer acknowledgement remain caller-owned.
    pub fn resolve(&self, budget: usize, mut ready: impl FnMut(usize) -> bool) -> Vec<SplatRange> {
        let mut selected: Vec<_> = self.leaves.iter().map(|leaf| leaf.old).collect();
        let mut count = selected
            .iter()
            .flatten()
            .fold(0usize, |sum, range| sum.saturating_add(range.count));
        for upgrade in [false, true] {
            for (index, leaf) in self.leaves.iter().enumerate() {
                let old_count = selected[index].map_or(0, |range| range.count);
                let new_count = leaf.target.map_or(0, |range| range.count);
                if (new_count > old_count) != upgrade || selected[index] == leaf.target {
                    continue;
                }
                if leaf
                    .target
                    .is_some_and(|range| range.count > 0 && !ready(range.file))
                {
                    continue;
                }
                let next_count = count.saturating_sub(old_count).saturating_add(new_count);
                if next_count <= budget || (!upgrade && next_count < count) {
                    selected[index] = leaf.target;
                    count = next_count;
                }
            }
        }
        selected
            .into_iter()
            .flatten()
            .filter(|range| range.count > 0)
            .collect()
    }
}
