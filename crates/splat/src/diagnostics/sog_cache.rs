//! Opt-in bounded source lifecycle history, not a network-byte or VRAM meter.
use bevy::{platform::time::Instant, prelude::*};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const HISTORY: usize = 128;
pub const GPU_EVIDENCE_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Request,
    Reload,
    Retry,
    Decoded,
    DecodeFailed,
    CpuAdmissionEviction,
    GpuAdmissionEviction,
    Uploaded,
    WarmReuse,
    WarmEviction,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub sequence: u64,
    pub elapsed_ms: f64,
    pub scene: String,
    pub source: usize,
    pub kind: Kind,
    /// CPU retained/charged GPU payload, not HTTP transfer bytes.
    pub bytes: u64,
    /// File whose admission caused the CPU eviction; GPU submits can contain many.
    pub incoming: Option<usize>,
    /// CPU events use the stream evaluation generation; GPU events use the
    /// adapter submission generation. These are not a shared join key.
    pub generation: u64,
    /// Latest CPU selection intent at GPU eviction, not the GPU generation.
    pub selection: Option<SelectionPlan>,
    /// Live packed admission, independent of the older CPU policy snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_admission: Option<GpuAdmission>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GpuAdmission {
    pub submitting_scene: String,
    pub budget_bytes: u64,
    pub resident_bytes_before: u64,
    /// Includes missing requested inputs and partial allocations, palettes once.
    pub needed_bytes_before: u64,
    pub requested: Vec<usize>,
    pub sources: Vec<GpuSourceOwner>,
    /// These lists are untruncated, not a full allocation breakdown: totals
    /// also include partial uploads and palette sharing, absent from `sources`.
    pub complete: bool,
}

/// IDs/counts only: this evidence must never acquire a GPU or CPU data owner.
#[derive(Clone, Debug, Serialize)]
pub struct GpuSourceOwner {
    pub scene: String,
    pub source: usize,
    pub last_used_generation: u64,
    pub exclusive_bytes: u64,
    pub external_owners: usize,
    pub fixed: bool,
    pub preparing: bool,
    pub requested: bool,
    pub evictable: bool,
}

/// Bounded evidence for pressure swaps. No pixel/source ownership is retained.
#[derive(Clone, Debug, Serialize)]
pub struct SelectionPlan {
    pub generation: u64,
    pub elapsed_ms: f64,
    pub eye: [f32; 3],
    pub desired: Vec<usize>,
    pub active: Vec<usize>,
    pub loading: Vec<usize>,
    pub gpu_costs: Vec<usize>,
    pub complete: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub events_total: u64,
    pub dropped_events: u64,
    pub recent: VecDeque<Event>,
    pub activation: ActivationReport,
}

/// Whole-file leaf targets only; timestamps share the source-history clock.
#[derive(Clone, Debug, Serialize)]
pub struct ActivationReport {
    pub scope: &'static str,
    pub events_total: u64,
    pub dropped_events: u64,
    pub untracked_leaf_observations: u64,
    pub recent: VecDeque<ActivationEvent>,
}

impl Default for ActivationReport {
    fn default() -> Self {
        Self {
            scope: "whole_file_leaf_targets_main_world_readiness",
            events_total: 0,
            dropped_events: 0,
            untracked_leaf_observations: 0,
            recent: VecDeque::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationKind {
    Target,
    SourceReady,
    Submitted,
    Activated,
    Cancelled,
    Guard,
}

/// An observed stream branch, not proof of a target's sole causal blocker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationGuard {
    RendererPending,
    UploadPreparing,
    SubmissionError,
    PolicyWait,
    ReplacementUnchanged,
    CountAdmission,
    SourceAdmission,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct GuardObservation {
    pub reason: ActivationGuard,
    /// Current policy generation; episode remains the target's original demand.
    pub generation: u64,
    pub selected: bool,
    pub source_ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// Expanded adapters do not expose enough residency evidence here.
    UnclassifiedBackend,
    GpuResident,
    CpuReady,
    CpuWarm,
    InFlight,
    Cold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ActivationRange {
    pub file: usize,
    pub offset: usize,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActivationEvent {
    pub sequence: u64,
    pub elapsed_ms: f64,
    pub scene: String,
    /// Join using (scene, leaf, episode), never CPU/GPU generation counters.
    pub episode: u64,
    pub leaf: usize,
    pub lod: usize,
    pub range: ActivationRange,
    pub availability: Availability,
    pub kind: ActivationKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<GuardObservation>,
}

struct Recorder {
    started: Instant,
    report: Report,
    plans: VecDeque<(Entity, SelectionPlan)>,
}

#[derive(Resource, Clone, Default)]
pub struct SogCacheTrace(Option<Arc<Mutex<Recorder>>>);

#[derive(Clone, Default)]
pub struct SceneTrace {
    recorder: SogCacheTrace,
    scene: Option<Entity>,
}

impl SogCacheTrace {
    /// Create or inspect an opt-in bounded diagnostic collector.
    pub fn enabled() -> Self {
        Self(Some(Arc::new(Mutex::new(Recorder {
            started: Instant::now(),
            report: Report::default(),
            plans: default(),
        }))))
    }
    /// Attach a diagnostic collector to one scene identity.
    pub fn scene(&self, scene: Entity) -> SceneTrace {
        SceneTrace {
            recorder: self.clone(),
            scene: Some(scene),
        }
    }
    /// Return the latest bounded accounting observations.
    pub fn report(&self) -> Option<Report> {
        self.0.as_ref()?.lock().ok().map(|r| r.report.clone())
    }
}

impl SceneTrace {
    /// Create or inspect an opt-in bounded diagnostic collector.
    pub fn enabled(&self) -> bool {
        self.recorder.0.is_some() && self.scene.is_some()
    }

    /// Record a bounded activation event when tracing is enabled.
    pub fn activation(&self, mut event: ActivationEvent) {
        let (Some(recorder), Some(scene)) = (&self.recorder.0, self.scene) else {
            return;
        };
        let Ok(mut recorder) = recorder.lock() else {
            return;
        };
        event.elapsed_ms = recorder.started.elapsed().as_secs_f64() * 1000.;
        event.scene = scene.to_bits().to_string();
        let report = &mut recorder.report.activation;
        report.events_total = report.events_total.saturating_add(1);
        event.sequence = report.events_total;
        if report.recent.len() == 2048 {
            report.recent.pop_front();
            report.dropped_events = report.dropped_events.saturating_add(1);
        }
        report.recent.push_back(event);
    }

    /// Count leaves omitted from the bounded trace.
    pub fn untracked_leaves(&self, count: usize) {
        if count == 0 {
            return;
        }
        if let Some(recorder) = &self.recorder.0
            && let Ok(mut recorder) = recorder.lock()
        {
            let report = &mut recorder.report.activation;
            report.untracked_leaf_observations = report
                .untracked_leaf_observations
                .saturating_add(count as u64);
        }
    }

    /// Record one bounded selection plan when tracing is enabled.
    pub fn selection(&self, make: impl FnOnce() -> SelectionPlan) {
        let (Some(recorder), Some(scene)) = (&self.recorder.0, self.scene) else {
            return;
        };
        let Ok(mut recorder) = recorder.lock() else {
            return;
        };
        let mut plan = make();
        plan.elapsed_ms = recorder.started.elapsed().as_secs_f64() * 1000.;
        for files in [
            &mut plan.desired,
            &mut plan.active,
            &mut plan.loading,
            &mut plan.gpu_costs,
        ] {
            plan.complete &= files.len() <= 256;
            files.truncate(256);
        }
        // Latest four evaluated scenes only; repeated scene loads cannot grow
        // this recorder indefinitely. Older eviction events own their snapshot.
        recorder.plans.retain(|(owner, _)| *owner != scene);
        if recorder.plans.len() == 4 {
            recorder.plans.pop_front();
        }
        recorder.plans.push_back((scene, plan));
    }

    /// Record one cache ownership event when tracing is enabled.
    pub fn record(
        &self,
        kind: Kind,
        source: usize,
        bytes: u64,
        incoming: Option<usize>,
        generation: u64,
    ) {
        self.record_with_gpu(kind, source, bytes, incoming, generation, || None);
    }

    /// Record retirement of immutable GPU source ownership.
    pub fn gpu_eviction(
        &self,
        source: usize,
        bytes: u64,
        generation: u64,
        make: impl FnOnce() -> GpuAdmission,
    ) {
        self.record_with_gpu(
            Kind::GpuAdmissionEviction,
            source,
            bytes,
            None,
            generation,
            || {
                let mut evidence = make();
                evidence.complete &= evidence.requested.len() <= GPU_EVIDENCE_LIMIT
                    && evidence.sources.len() <= GPU_EVIDENCE_LIMIT;
                evidence.requested.truncate(GPU_EVIDENCE_LIMIT);
                evidence.sources.truncate(GPU_EVIDENCE_LIMIT);
                Some(evidence)
            },
        );
    }

    fn record_with_gpu(
        &self,
        kind: Kind,
        source: usize,
        bytes: u64,
        incoming: Option<usize>,
        generation: u64,
        make: impl FnOnce() -> Option<GpuAdmission>,
    ) {
        let (Some(recorder), Some(scene)) = (&self.recorder.0, self.scene) else {
            return;
        };
        let Ok(mut recorder) = recorder.lock() else {
            return;
        };
        let gpu_admission = make();
        let elapsed_ms = recorder.started.elapsed().as_secs_f64() * 1000.;
        let selection = (kind == Kind::GpuAdmissionEviction)
            .then(|| {
                recorder
                    .plans
                    .iter()
                    .find(|(owner, _)| *owner == scene)
                    .map(|(_, plan)| plan.clone())
            })
            .flatten();
        let report = &mut recorder.report;
        report.events_total = report.events_total.saturating_add(1);
        if report.recent.len() == HISTORY {
            report.recent.pop_front();
            report.dropped_events = report.dropped_events.saturating_add(1);
        }
        let sequence = report.events_total;
        report.recent.push_back(Event {
            sequence,
            elapsed_ms,
            scene: scene.to_bits().to_string(),
            source,
            kind,
            bytes,
            incoming,
            generation,
            selection,
            gpu_admission,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_history_preserves_the_decision_at_eviction_not_a_later_plan() {
        let trace = SogCacheTrace::enabled();
        let mut world = World::new();
        let a = trace.scene(world.spawn_empty().id());
        let b = trace.scene(world.spawn_empty().id());
        let plan = |generation, desired| SelectionPlan {
            generation,
            elapsed_ms: 0.,
            eye: [1., 2., 3.],
            desired: vec![desired],
            active: vec![26],
            loading: vec![17],
            gpu_costs: vec![100; 47],
            complete: true,
        };
        a.selection(|| plan(61, 41));
        b.selection(|| plan(7, 9));
        a.record(Kind::GpuAdmissionEviction, 41, 12859824, None, 38);
        a.selection(|| plan(63, 17));
        a.record(Kind::Reload, 41, 0, None, 63);
        let report = trace.report().unwrap();
        let eviction = &report.recent[0];
        let selection = eviction
            .selection
            .as_ref()
            .expect("eviction must retain selection intent");
        assert_eq!(selection.generation, 61);
        assert_eq!(selection.desired, [41]);
        assert_eq!(selection.active, [26]);
        assert_eq!(selection.loading, [17]);
        assert!(report.recent[1].selection.is_none());
    }

    #[test]
    fn disabled_pressure_diagnostics_do_not_construct_file_lists() {
        SogCacheTrace::default()
            .scene(Entity::PLACEHOLDER)
            .selection(|| panic!("disabled diagnostics must not allocate a plan"));
        SogCacheTrace::default()
            .scene(Entity::PLACEHOLDER)
            .gpu_eviction(1, 42, 7, || {
                panic!("disabled diagnostics must not inspect GPU owners")
            });
    }

    #[test]
    fn live_gpu_evidence_is_bounded_and_does_not_replace_cpu_intent() {
        let trace = SogCacheTrace::enabled();
        let mut world = World::new();
        let scene = world.spawn_empty().id();
        let collector = trace.scene(scene);
        collector.gpu_eviction(8, 72, 9, || GpuAdmission {
            submitting_scene: scene.to_bits().to_string(),
            budget_bytes: 8080,
            resident_bytes_before: 8080,
            needed_bytes_before: 8152,
            requested: (0..100).collect(),
            sources: (0..100)
                .map(|source| GpuSourceOwner {
                    scene: scene.to_bits().to_string(),
                    source,
                    last_used_generation: 3,
                    exclusive_bytes: 72,
                    external_owners: 0,
                    fixed: false,
                    preparing: false,
                    requested: false,
                    evictable: true,
                })
                .collect(),
            complete: true,
        });
        collector.record(Kind::Reload, 8, 0, None, 4);
        let report = trace.report().unwrap();
        let eviction = &report.recent[0];
        assert!(
            eviction.selection.is_none(),
            "no CPU plan must be fabricated"
        );
        let evidence = eviction.gpu_admission.as_ref().unwrap();
        assert!(!evidence.complete, "truncation must be explicit");
        assert_eq!(evidence.sources.len(), 64);
        assert_eq!(evidence.requested.len(), 64);
        assert_eq!(evidence.needed_bytes_before, 8152);
        assert!(report.recent[1].gpu_admission.is_none());
    }

    #[test]
    fn pressure_plans_bound_file_lists_and_scene_lifetime() {
        let trace = SogCacheTrace::enabled();
        let mut world = World::new();
        let scenes: Vec<_> = (0..5).map(|_| world.spawn_empty().id()).collect();
        for &scene in &scenes {
            trace.scene(scene).selection(|| SelectionPlan {
                generation: 1,
                elapsed_ms: 0.,
                eye: [0.; 3],
                desired: (0..300).collect(),
                active: vec![],
                loading: vec![],
                gpu_costs: vec![42; 300],
                complete: true,
            });
        }
        trace
            .scene(scenes[0])
            .record(Kind::GpuAdmissionEviction, 2, 42, None, 1);
        trace
            .scene(scenes[4])
            .record(Kind::GpuAdmissionEviction, 2, 42, None, 1);
        let report = trace.report().unwrap();
        assert!(
            report.recent[0].selection.is_none(),
            "expired scene evidence must not be invented"
        );
        let plan = report.recent[1].selection.as_ref().unwrap();
        assert_eq!(plan.desired.len(), 256);
        assert_eq!(plan.gpu_costs.len(), 256);
        assert!(
            !plan.complete,
            "truncation cannot claim complete pressure evidence"
        );
    }

    #[test]
    fn lifecycle_history_is_bounded_and_preserves_scene_reason_and_sequence() {
        let trace = SogCacheTrace::enabled();
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let first = trace.scene(a);
        let second = trace.scene(b);
        for _ in 0..128 {
            first.record(Kind::Request, 3, 42, None, 7);
        }
        second.record(Kind::CpuAdmissionEviction, 8, 64, Some(9), 11);
        drop(first);
        let report = trace.report().unwrap();
        assert_eq!(report.events_total, 129);
        assert_eq!(report.dropped_events, 1);
        assert_eq!(report.recent.len(), 128);
        assert_eq!(report.recent.front().unwrap().sequence, 2);
        let last = report.recent.back().unwrap();
        assert_eq!(last.kind, Kind::CpuAdmissionEviction);
        assert_eq!(last.scene, b.to_bits().to_string());
        assert_eq!(
            (last.source, last.bytes, last.incoming, last.generation),
            (8, 64, Some(9), 11)
        );
    }

    #[test]
    fn disabled_history_is_unavailable_not_an_empty_measurement() {
        let trace = SogCacheTrace::default();
        let mut world = World::new();
        trace
            .scene(world.spawn_empty().id())
            .record(Kind::Reload, 2, 3, None, 4);
        assert!(trace.report().is_none());
    }
}
