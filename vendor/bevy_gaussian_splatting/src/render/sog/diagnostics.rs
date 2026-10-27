//! Opt-in aggregate CPU encoding work, not GPU duration or completed commands.
//! No histories, camera/source identities, or GPU owners are retained.

use bevy::prelude::Resource;
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct BakeWorkReport {
    /// Attempts after local-view preparation, including failed encodings.
    pub full_updates: u64,
    /// Attempts after local-view preparation, including failed encodings.
    pub color_updates: u64,
    pub cache_hits: u64,
    /// Dispatch-limit/encoding failures only; excludes invalid local-view setup.
    pub failed_updates: u64,
    pub full_rows: u64,
    pub color_rows: u64,
    /// Unchanged rows copied between already-admitted working slots.
    pub copied_rows: u64,
    pub copied_bytes: u64,
    pub dispatches: u64,
    pub uniform_buffers: u64,
    pub uniform_bytes: u64,
    pub bind_groups: u64,
    pub compute_passes: u64,
}

/// Clone into main/render worlds to share one fixed-size recorder. Missing or
/// disabled telemetry reports `None`; an enabled unused recorder reports zeros.
#[derive(Resource, Clone, Default)]
pub struct BakeDiagnostics(Option<Arc<Mutex<BakeWorkReport>>>);

impl BakeDiagnostics {
    pub fn enabled() -> Self {
        Self(Some(Arc::new(Mutex::new(BakeWorkReport::default()))))
    }

    pub fn report(&self) -> Option<BakeWorkReport> {
        self.0.as_ref()?.lock().ok().map(|report| *report)
    }

    pub(super) fn record(&self, update: impl FnOnce(&mut BakeWorkReport)) {
        if let Some(recorder) = &self.0 {
            if let Ok(mut report) = recorder.lock() {
                update(&mut report);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_bake_diagnostics_do_not_construct_measurements() {
        let disabled = BakeDiagnostics::default();
        disabled.record(|_| panic!("disabled recorder evaluated measurement"));
        assert!(disabled.report().is_none());
    }

    #[test]
    fn render_world_updates_reach_main_world_without_mutating_old_reports() {
        let main = BakeDiagnostics::enabled();
        let render = main.clone();
        let before = main.report().unwrap();
        render.record(|report| report.full_rows += 7);
        assert_eq!(before.full_rows, 0);
        assert_eq!(main.report().unwrap().full_rows, 7);
    }
}
