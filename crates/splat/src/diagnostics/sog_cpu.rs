//! Opt-in, fixed-size elapsed CPU-stage accounting for streamed SOG.
//! These are monotonic wall intervals, not thread CPU or GPU execution time.

use bevy::{platform::time::Instant, prelude::*};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
pub enum CpuStage {
    Decode,
    SnapshotCompose,
    RelocationPlan,
    SelectionPlan,
    SourceUploadPrepare,
    SelectionCopyPrepare,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct StageTotal {
    pub attempts: u64,
    pub total_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

impl StageTotal {
    fn record(&mut self, elapsed_ms: f64) {
        self.attempts = self.attempts.saturating_add(1);
        self.total_ms = Some(self.total_ms.unwrap_or(0.0) + elapsed_ms);
        self.max_ms = Some(self.max_ms.unwrap_or(0.0).max(elapsed_ms));
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CpuStages {
    pub decode: StageTotal,
    pub snapshot_compose: StageTotal,
    pub relocation_plan: StageTotal,
    pub selection_plan: StageTotal,
    pub source_upload_prepare: StageTotal,
    pub selection_copy_prepare: StageTotal,
}

impl CpuStages {
    fn stage_mut(&mut self, stage: CpuStage) -> &mut StageTotal {
        match stage {
            CpuStage::Decode => &mut self.decode,
            CpuStage::SnapshotCompose => &mut self.snapshot_compose,
            CpuStage::RelocationPlan => &mut self.relocation_plan,
            CpuStage::SelectionPlan => &mut self.selection_plan,
            CpuStage::SourceUploadPrepare => &mut self.source_upload_prepare,
            CpuStage::SelectionCopyPrepare => &mut self.selection_copy_prepare,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CpuTimingReport {
    pub cpu_stages: Option<CpuStages>,
    pub cpu_timing_unavailable_reason: Option<&'static str>,
}

impl Default for CpuTimingReport {
    fn default() -> Self {
        Self {
            cpu_stages: None,
            cpu_timing_unavailable_reason: Some("performance telemetry disabled"),
        }
    }
}

/// One recorder per application run, shared with loaders without retaining ECS
/// entities. Six aggregates only: no per-frame/file histories or growing maps.
#[derive(Resource, Clone, Default)]
pub struct SogCpuTimings(Option<Arc<Mutex<CpuStages>>>);

impl SogCpuTimings {
    /// Create or inspect an opt-in bounded diagnostic collector.
    pub fn enabled() -> Self {
        Self(Some(Arc::new(Mutex::new(CpuStages::default()))))
    }

    /// Begin a bounded codec job or timing interval.
    pub fn start(&self, stage: CpuStage) -> Option<StageTimer> {
        self.0.as_ref().map(|stages| StageTimer {
            stages: Arc::clone(stages),
            stage,
            started: Instant::now(),
        })
    }

    /// Result errors are measured too; this is attempted work, not throughput.
    pub fn measure<T>(&self, stage: CpuStage, work: impl FnOnce() -> T) -> T {
        let _timer = self.start(stage);
        work()
    }

    /// Return the latest bounded accounting observations.
    pub fn report(&self) -> CpuTimingReport {
        let Some(stages) = &self.0 else {
            return CpuTimingReport::default();
        };
        match stages.lock() {
            Ok(stages) => CpuTimingReport {
                cpu_stages: Some(stages.clone()),
                cpu_timing_unavailable_reason: None,
            },
            Err(_) => CpuTimingReport {
                cpu_stages: None,
                cpu_timing_unavailable_reason: Some("CPU timing recorder poisoned"),
            },
        }
    }
}

pub struct StageTimer {
    stages: Arc<Mutex<CpuStages>>,
    stage: CpuStage,
    started: Instant,
}

impl Drop for StageTimer {
    fn drop(&mut self) {
        // Measure before acquiring the recorder lock. Never hold it across work.
        let elapsed_ms = self.started.elapsed().as_secs_f64() * 1_000.0;
        if let Ok(mut stages) = self.stages.lock() {
            stages.stage_mut(self.stage).record(elapsed_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_recorder_executes_work_without_reporting_zero_time() {
        let recorder = SogCpuTimings::default();
        let mut value = 0;
        recorder.measure(CpuStage::Decode, || value += 1);
        assert_eq!(value, 1);
        assert!(recorder.start(CpuStage::Decode).is_none());
        assert!(recorder.report().cpu_stages.is_none());
    }

    #[test]
    fn measured_errors_and_retired_loader_handles_remain_in_run_totals() {
        let recorder = SogCpuTimings::enabled();
        let loader = recorder.clone();
        let result: Result<(), &str> = loader.measure(CpuStage::Decode, || Err("bad texture"));
        assert_eq!(result, Err("bad texture"));
        drop(loader);
        recorder.measure(CpuStage::Decode, || ());
        let report = recorder.report().cpu_stages.expect("enabled");
        assert_eq!(report.decode.attempts, 2);
        assert!(
            report.decode.total_ms.expect("measured") >= report.decode.max_ms.expect("measured")
        );
        assert_eq!(report.snapshot_compose.attempts, 0);
        assert!(report.snapshot_compose.total_ms.is_none());
    }

    #[test]
    fn stage_totals_use_milliseconds_and_keep_max_separate_from_sum() {
        let mut total = StageTotal::default();
        total.record(0.25);
        total.record(2.0);
        total.record(0.0); // Quantized zero is an observation, not unavailable.
        assert_eq!(total.attempts, 3);
        assert_eq!(total.total_ms, Some(2.25));
        assert_eq!(total.max_ms, Some(2.0));
        assert!(
            SogCpuTimings::enabled()
                .report()
                .cpu_timing_unavailable_reason
                .is_none()
        );
    }
}
