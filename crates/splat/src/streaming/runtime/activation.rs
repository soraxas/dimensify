//! Opt-in leaf activation evidence. No handles, policy changes or redraw demand.
use super::{SplatRange, cache::Entry, manifest::Manifest};
use crate::diagnostics::sog_cache::{
    ActivationEvent, ActivationGuard as Guard, ActivationKind as Kind, ActivationRange,
    Availability, GuardObservation, SceneTrace,
};
use crate::renderer::source::{SourceData, SourceStorage};

const MAX_LEAVES: usize = 256;

struct Target {
    episode: u64,
    lod: usize,
    range: SplatRange,
    availability: Availability,
    source_ready: bool,
    submitted: bool,
    activated: bool,
    last_guard: Option<(Guard, bool, bool)>,
}

#[cfg(test)]
mod tests {
    use super::super::tests::tiny_stream;
    use super::*;
    use crate::diagnostics::sog_cache::SogCacheTrace;
    use bevy::prelude::*;

    #[test]
    fn guard_changes_are_joinable_deduplicated_and_stop_after_submission() {
        let stream = tiny_stream();
        let range = stream.manifest.leaves[0].lods[0].1;
        let trace = SogCacheTrace::enabled();
        let scene = trace.scene(Entity::PLACEHOLDER);
        let mut tracker = Tracker::default();
        tracker.observe(
            &scene,
            7,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        for generation in 7..107 {
            tracker.guard(
                &scene,
                generation,
                Guard::RendererPending,
                &[],
                &stream.entries,
            );
        }
        tracker.guard(
            &scene,
            107,
            Guard::UploadPreparing,
            &[range],
            &stream.entries,
        );
        tracker.submitted(&scene, &[range]);
        tracker.guard(&scene, 108, Guard::PolicyWait, &[range], &stream.entries);
        let events = trace.report().unwrap().activation.recent;
        let guards: Vec<_> = events.iter().filter(|e| e.kind == Kind::Guard).collect();
        assert_eq!(
            guards.len(),
            2,
            "record branch changes, not every frame/generation"
        );
        for event in &guards {
            assert_eq!(event.scene, Entity::PLACEHOLDER.to_bits().to_string());
            assert_eq!((event.episode, event.leaf), (7, 0));
            assert_eq!(
                (event.range.file, event.range.offset, event.range.count),
                (range.file, range.offset, range.count)
            );
            assert!(event.elapsed_ms >= events[0].elapsed_ms);
        }
        assert_eq!(guards[0].guard.unwrap().reason, Guard::RendererPending);
        assert!(!guards[0].guard.unwrap().selected);
        assert_eq!(guards[1].guard.unwrap().generation, 107);
        assert!(guards[1].guard.unwrap().selected);
        assert!(guards[1].guard.unwrap().source_ready);
    }

    #[test]
    fn returning_to_a_displaced_target_starts_a_new_episode() {
        let stream = tiny_stream();
        let range = stream.manifest.leaves[0].lods[0].1;
        let trace = SogCacheTrace::enabled();
        let scene = trace.scene(Entity::PLACEHOLDER);
        let mut tracker = Tracker::default();
        tracker.observe(
            &scene,
            1,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        tracker.submitted(&scene, &[range]);
        tracker.activated(&scene, &[range]);
        tracker.observe(
            &scene,
            2,
            &stream.manifest,
            &[Some(range)],
            &[range],
            &stream.entries,
        );
        tracker.observe(
            &scene,
            3,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        tracker.submitted(&scene, &[range]);
        tracker.activated(&scene, &[range]);
        let events = trace.report().unwrap().activation.recent;
        assert_eq!(
            events.len(),
            6,
            "a displaced target needs a new activation interval"
        );
        assert_eq!(events[3].episode, 3);
    }

    #[test]
    fn replacement_cancels_old_episode_and_does_not_match_a_shared_file() {
        let mut stream = tiny_stream();
        let first = stream.manifest.leaves[0].lods[0].1;
        let second = SplatRange { offset: 1, ..first };
        stream.manifest.leaves[0].lods.insert(0, (1, second));
        let trace = SogCacheTrace::enabled();
        let scene = trace.scene(Entity::PLACEHOLDER);
        let mut tracker = Tracker::default();
        tracker.observe(
            &scene,
            1,
            &stream.manifest,
            &[Some(first)],
            &[],
            &stream.entries,
        );
        tracker.submitted(&scene, &[first]);
        tracker.observe(
            &scene,
            2,
            &stream.manifest,
            &[Some(second)],
            &[],
            &stream.entries,
        );
        tracker.activated(&scene, &[first]);
        tracker.submitted(&scene, &[first]);
        tracker.activated(&scene, &[second]); // Not submitted: not a valid acknowledgement.
        tracker.submitted(&scene, &[second]);
        tracker.activated(&scene, &[second]);
        let events = trace.report().unwrap().activation.recent;
        assert_eq!(
            events.iter().map(|e| e.kind).collect::<Vec<_>>(),
            [
                Kind::Target,
                Kind::Submitted,
                Kind::Cancelled,
                Kind::Target,
                Kind::Submitted,
                Kind::Activated
            ]
        );
        assert_eq!(events.back().unwrap().range.offset, 1);
        assert_eq!(events.back().unwrap().episode, 2);
    }

    #[test]
    fn disabled_tracker_allocates_no_leaf_history() {
        let stream = tiny_stream();
        let mut tracker = Tracker::default();
        tracker.observe(
            &SceneTrace::default(),
            1,
            &stream.manifest,
            &[Some(stream.manifest.leaves[0].lods[0].1)],
            &[],
            &stream.entries,
        );
        tracker.guard(
            &SceneTrace::default(),
            1,
            Guard::PolicyWait,
            &[],
            &stream.entries,
        );
        assert_eq!(tracker.targets.capacity(), 0);
    }

    #[test]
    fn guard_history_is_bounded_and_reports_missing_endpoints() {
        let stream = tiny_stream();
        let range = stream.manifest.leaves[0].lods[0].1;
        let trace = SogCacheTrace::enabled();
        let scene = trace.scene(Entity::PLACEHOLDER);
        let mut tracker = Tracker::default();
        tracker.observe(
            &scene,
            1,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        for generation in 0..2200 {
            let reason = if generation % 2 == 0 {
                Guard::RendererPending
            } else {
                Guard::UploadPreparing
            };
            tracker.guard(&scene, generation, reason, &[], &stream.entries);
        }
        let report = trace.report().unwrap().activation;
        assert_eq!(report.events_total, 2201);
        assert_eq!(report.recent.len(), 2048);
        assert_eq!(report.dropped_events, 153);
        assert_eq!(report.recent.front().unwrap().sequence, 154);
        // Observing guards does not manufacture submission/readiness progress.
        assert!(report.recent.iter().all(|event| event.kind == Kind::Guard));
        assert!(!tracker.targets[0].as_ref().unwrap().submitted);
        assert!(!tracker.targets[0].as_ref().unwrap().activated);
    }

    #[test]
    fn classifies_first_demand_without_retaining_sources_and_records_readiness_once() {
        use crate::renderer::{DecodedChunk, source::PackedSourceDescriptor};
        let mut stream = tiny_stream();
        let range = stream.manifest.leaves[0].lods[0].1;
        let trace = SogCacheTrace::enabled();
        let scene = trace.scene(Entity::PLACEHOLDER);
        let mut tracker = Tracker {
            storage: SourceStorage::Packed,
            ..Default::default()
        };
        stream.entries[0].ready = None;
        tracker.observe(
            &scene,
            1,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        tracker.source_ready(&scene, 0);
        tracker.source_ready(&scene, 0);
        tracker.observe(&scene, 2, &stream.manifest, &[None], &[], &stream.entries);
        stream.entries[0].warm = Some(DecodedChunk {
            cloud: Default::default(),
            count: 1,
        });
        tracker.observe(
            &scene,
            3,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        tracker.observe(&scene, 4, &stream.manifest, &[None], &[], &stream.entries);
        stream.entries[0].ready = Some(DecodedChunk {
            cloud: SourceData::ResidentPacked(PackedSourceDescriptor {
                count: 1,
                group: (false, 0),
                gpu_bytes: 42,
            }),
            count: 1,
        });
        tracker.observe(
            &scene,
            5,
            &stream.manifest,
            &[Some(range)],
            &[],
            &stream.entries,
        );
        let events = trace.report().unwrap().activation.recent;
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == Kind::Target)
                .map(|e| e.availability)
                .collect::<Vec<_>>(),
            [
                Availability::Cold,
                Availability::CpuWarm,
                Availability::GpuResident
            ]
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == Kind::SourceReady)
                .count(),
            1
        );
        assert_eq!(
            events.iter().filter(|e| e.kind == Kind::Activated).count(),
            0
        );
    }

    #[test]
    fn bounded_history_reports_loss_and_leaf_truncation() {
        let stream = tiny_stream();
        let range = stream.manifest.leaves[0].lods[0].1;
        let trace = SogCacheTrace::enabled();
        let scene = trace.scene(Entity::PLACEHOLDER);
        let mut tracker = Tracker::default();
        for episode in 0..1100 {
            tracker.observe(
                &scene,
                episode,
                &stream.manifest,
                &[Some(range)],
                &[],
                &stream.entries,
            );
            tracker.observe(
                &scene,
                episode,
                &stream.manifest,
                &[None],
                &[],
                &stream.entries,
            );
        }
        tracker.observe(
            &scene,
            1100,
            &stream.manifest,
            &vec![None; 300],
            &[],
            &stream.entries,
        );
        let report = trace.report().unwrap().activation;
        assert_eq!(report.events_total, 2200);
        assert_eq!(report.recent.len(), 2048);
        assert_eq!(report.dropped_events, 152);
        assert_eq!(report.recent.front().unwrap().sequence, 153);
        assert_eq!(report.untracked_leaf_observations, 44);
        assert_eq!(tracker.targets.len(), 256);
    }
}

impl Target {
    fn record(&self, trace: &SceneTrace, leaf: usize, kind: Kind) {
        trace.activation(ActivationEvent {
            sequence: 0,
            elapsed_ms: 0.,
            scene: String::new(),
            episode: self.episode,
            leaf,
            lod: self.lod,
            range: ActivationRange {
                file: self.range.file,
                offset: self.range.offset,
                count: self.range.count,
            },
            availability: self.availability,
            kind,
            guard: None,
        });
    }
}

#[derive(Default)]
pub(super) struct Tracker {
    pub(super) storage: SourceStorage,
    targets: Vec<Option<Target>>,
}

impl Tracker {
    /// Check whether activation is blocked by an ownership or readiness guard.
    pub(super) fn guard(
        &mut self,
        trace: &SceneTrace,
        generation: u64,
        reason: Guard,
        selected: &[SplatRange],
        entries: &[Entry],
    ) {
        if !trace.enabled() {
            return;
        }
        for (leaf, target) in self.targets.iter_mut().enumerate() {
            let Some(target) = target else { continue };
            if target.activated || target.submitted {
                continue;
            }
            let selected = selected.contains(&target.range);
            let source_ready = entries[target.range.file].ready.is_some();
            let state = (reason, selected, source_ready);
            if target.last_guard == Some(state) {
                continue;
            }
            target.last_guard = Some(state);
            trace.activation(ActivationEvent {
                sequence: 0,
                elapsed_ms: 0.,
                scene: String::new(),
                episode: target.episode,
                leaf,
                lod: target.lod,
                range: ActivationRange {
                    file: target.range.file,
                    offset: target.range.offset,
                    count: target.range.count,
                },
                availability: target.availability,
                kind: Kind::Guard,
                guard: Some(GuardObservation {
                    reason,
                    generation,
                    selected,
                    source_ready,
                }),
            });
        }
    }

    /// Record bounded activation evidence for the current generation.
    pub(super) fn observe(
        &mut self,
        trace: &SceneTrace,
        episode: u64,
        manifest: &Manifest,
        desired: &[Option<SplatRange>],
        displayed: &[SplatRange],
        entries: &[Entry],
    ) {
        if !trace.enabled() {
            return;
        }
        trace.untracked_leaves(desired.len().saturating_sub(MAX_LEAVES));
        self.targets
            .resize_with(desired.len().min(MAX_LEAVES), || None);
        for (leaf, (slot, range)) in self.targets.iter_mut().zip(desired).enumerate() {
            let range = range.filter(|r| r.count > 0);
            if slot.as_ref().is_some_and(|old| {
                Some(old.range) == range && (!old.activated || displayed.contains(&old.range))
            }) {
                continue;
            }
            if let Some(old) = slot.take()
                && !old.activated
            {
                old.record(trace, leaf, Kind::Cancelled);
            }
            let Some(range) = range else {
                continue;
            };
            // Already displayed at first observation is not a zero-latency sample.
            let activated = displayed.contains(&range);
            let entry = &entries[range.file];
            let availability = if self.storage != SourceStorage::Packed {
                Availability::UnclassifiedBackend
            } else {
                match entry.ready.as_ref().map(|r| &r.cloud) {
                    Some(SourceData::ResidentPacked(_)) => Availability::GpuResident,
                    Some(_) => Availability::CpuReady,
                    None if entry.warm.is_some() => Availability::CpuWarm,
                    None if entry.loading.is_some() => Availability::InFlight,
                    None => Availability::Cold,
                }
            };
            let target = Target {
                episode,
                range,
                availability,
                lod: manifest.leaves[leaf]
                    .lods
                    .iter()
                    .find(|(_, r)| *r == range)
                    .map_or(0, |(lod, _)| *lod),
                source_ready: entry.ready.is_some(),
                submitted: false,
                activated,
                last_guard: None,
            };
            if !activated {
                target.record(trace, leaf, Kind::Target);
            }
            *slot = Some(target);
        }
    }

    /// Record completion of an immutable source.
    pub(super) fn source_ready(&mut self, trace: &SceneTrace, file: usize) {
        if !trace.enabled() {
            return;
        }
        for (leaf, target) in self.targets.iter_mut().enumerate() {
            if let Some(target) = target
                && target.range.file == file
                && !target.source_ready
                && !target.activated
            {
                target.source_ready = true;
                target.record(trace, leaf, Kind::SourceReady);
            }
        }
    }

    /// Record accepted selection ownership before renderer readiness.
    pub(super) fn submitted(&mut self, trace: &SceneTrace, ranges: &[SplatRange]) {
        if !trace.enabled() {
            return;
        }
        for (leaf, target) in self.targets.iter_mut().enumerate() {
            if let Some(target) = target
                && !target.activated
                && !target.submitted
                && ranges.contains(&target.range)
            {
                target.submitted = true;
                target.record(trace, leaf, Kind::Submitted);
            }
        }
    }

    /// Record a renderer-ready generation activation.
    pub(super) fn activated(&mut self, trace: &SceneTrace, ranges: &[SplatRange]) {
        if !trace.enabled() {
            return;
        }
        for (leaf, target) in self.targets.iter_mut().enumerate() {
            if let Some(target) = target
                && target.submitted
                && !target.activated
                && ranges.contains(&target.range)
            {
                target.activated = true;
                target.record(trace, leaf, Kind::Activated);
            }
        }
    }
}

impl super::SogStream {
    /// Record the current activation guard in the optional trace.
    pub(super) fn trace_guard(&mut self, reason: Guard) {
        self.activation.guard(
            &self.cache_trace,
            self.generation,
            reason,
            &self.active,
            &self.entries,
        );
    }
}
