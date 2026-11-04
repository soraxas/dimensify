//! Opt-in elapsed preparation timings for whole-file imports. No GPU-time inference.
use bevy::{platform::time::Instant, prelude::*};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
pub(crate) enum LoadingStage {
    InputRead,
    InputValidation,
    ArchiveRead,
    Allocation,
    TextureDecode,
    AttributeExpansion,
    PlaneTransfer,
    ValidationBounds,
    PlyDecode,
    ScenePrepare,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LoadingTotal {
    pub attempts: u64,
    pub total_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LoadingStages {
    pub input_read: LoadingTotal,
    pub input_validation: LoadingTotal,
    pub archive_read: LoadingTotal,
    pub allocation: LoadingTotal,
    pub texture_decode: LoadingTotal,
    pub attribute_expansion: LoadingTotal,
    pub plane_transfer: LoadingTotal,
    pub validation_bounds: LoadingTotal,
    pub ply_decode: LoadingTotal,
    pub scene_prepare: LoadingTotal,
}

impl LoadingStages {
    fn get_mut(&mut self, stage: LoadingStage) -> &mut LoadingTotal {
        match stage {
            LoadingStage::InputRead => &mut self.input_read,
            LoadingStage::InputValidation => &mut self.input_validation,
            LoadingStage::ArchiveRead => &mut self.archive_read,
            LoadingStage::Allocation => &mut self.allocation,
            LoadingStage::TextureDecode => &mut self.texture_decode,
            LoadingStage::AttributeExpansion => &mut self.attribute_expansion,
            LoadingStage::PlaneTransfer => &mut self.plane_transfer,
            LoadingStage::ValidationBounds => &mut self.validation_bounds,
            LoadingStage::PlyDecode => &mut self.ply_decode,
            LoadingStage::ScenePrepare => &mut self.scene_prepare,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WholeFileTimingReport {
    pub stages: Option<LoadingStages>,
    pub unavailable_reason: Option<&'static str>,
    pub semantics: &'static str,
    pub gpu_upload_elapsed_ms: Option<f64>,
    pub gpu_upload_unavailable_reason: &'static str,
}

/// Ten fixed-size totals, including failed/retired attempts; disabled by default.
/// Enable this resource before imports to collect preparation wall intervals.
#[derive(Resource, Clone, Default)]
pub struct WholeFileTimings(Option<Arc<Mutex<LoadingStages>>>);

impl WholeFileTimings {
    pub fn enabled() -> Self {
        Self(Some(Arc::new(Mutex::new(LoadingStages::default()))))
    }

    pub fn report(&self) -> WholeFileTimingReport {
        let stages = self
            .0
            .as_ref()
            .and_then(|stages| stages.lock().ok().map(|v| v.clone()));
        let unavailable_reason = if self.0.is_none() {
            Some("whole-file timing disabled")
        } else if stages.is_none() {
            Some("whole-file timing recorder poisoned")
        } else {
            None
        };
        WholeFileTimingReport {
            stages,
            unavailable_reason,
            semantics: "lifetime attempted wall intervals, not thread CPU; synchronous decode phases do not overlap; input_read includes IO await and byte copy but excludes picker wait; viewer metadata reads may also contribute; scene_prepare ends at main-world asset admission",
            gpu_upload_elapsed_ms: None,
            gpu_upload_unavailable_reason: "Bevy render-asset upload submission/completion is not isolated by these CPU preparation timers",
        }
    }

    pub(crate) fn start(&self, stage: LoadingStage) -> Option<LoadingTimer> {
        self.0.as_ref().map(|stages| LoadingTimer {
            stages: stages.clone(),
            stage,
            start: Instant::now(),
        })
    }

    pub(crate) fn measure<T>(&self, stage: LoadingStage, work: impl FnOnce() -> T) -> T {
        let _timer = self.start(stage);
        work()
    }
}

pub(crate) struct LoadingTimer {
    stages: Arc<Mutex<LoadingStages>>,
    stage: LoadingStage,
    start: Instant,
}

impl Drop for LoadingTimer {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_secs_f64() * 1000.;
        if let Ok(mut stages) = self.stages.lock() {
            let total = stages.get_mut(self.stage);
            total.attempts = total.attempts.saturating_add(1);
            total.total_ms = Some(total.total_ms.unwrap_or(0.) + ms);
            total.max_ms = Some(total.max_ms.unwrap_or(0.).max(ms));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_and_unattempted_work_remain_unavailable() {
        let disabled = WholeFileTimings::default();
        assert_eq!(disabled.measure(LoadingStage::InputRead, || 42), 42);
        assert!(disabled.report().stages.is_none());
        let enabled = WholeFileTimings::enabled();
        let clone = enabled.clone();
        assert!(
            clone
                .measure(LoadingStage::InputRead, || Err::<(), _>("read failed"))
                .is_err()
        );
        let report = enabled.report().stages.unwrap();
        assert_eq!(report.input_read.attempts, 1);
        assert!(report.input_read.total_ms.is_some());
        assert_eq!(report.texture_decode.attempts, 0);
        assert!(report.texture_decode.total_ms.is_none());
    }
}
