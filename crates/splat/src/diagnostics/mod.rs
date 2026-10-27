//! Optional bounded streaming diagnostics; no product publication or UI.
use serde::Serialize;
pub mod quality;
pub mod sog_cache;
pub mod sog_cpu;
/// Observed snapshot attribute/indirect buffers, not total VRAM or completed uploads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotGpuBufferReport {
    pub prepared_bytes_total: u64,
    pub prepared_buffers_total: u64,
    pub live_bytes: u64,
    pub live_buffers: usize,
    pub live_assets: usize,
}

/// Frame-observed streamed sorting allocations; not reservations or total VRAM.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct SortGpuAccountingReport {
    pub sort_gpu_buffers: Option<SortGpuBufferReport>,
    pub sort_gpu_buffers_unavailable_reason: Option<&'static str>,
}

impl SortGpuAccountingReport {
    /// Describe why allocation accounting is unavailable.
    pub fn unavailable(reason: &'static str) -> Self {
        Self {
            sort_gpu_buffers: None,
            sort_gpu_buffers_unavailable_reason: Some(reason),
        }
    }
}

impl Default for SortGpuAccountingReport {
    fn default() -> Self {
        Self::unavailable("performance telemetry disabled")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SortGpuBufferReport {
    pub sorted_entries: GpuBufferCapacityReport,
    pub radix_scratch: GpuBufferCapacityReport,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct GpuBufferCapacityReport {
    pub observed_bytes_total: u64,
    pub observed_buffers_total: u64,
    pub live_bytes: u64,
    pub live_buffers: usize,
    pub live_assets: usize,
}

/// Current streamed-SOG work, not differences of lifetime generation counters.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SogPerfActivity {
    pub active_streams: usize,
    pub awaiting_first_evaluation: usize,
    pub pending_downloads: usize,
    pub pending_generations: usize,
    pub refining_streams: usize,
    /// Logical render-ready count; not a GPU-visible or pixel-contribution count.
    pub displayed_splats: usize,
}

impl SogPerfActivity {
    /// Report whether downloads, preparation or activation require further work.
    pub fn is_busy(&self) -> bool {
        self.awaiting_first_evaluation > 0
            || self.pending_downloads > 0
            || self.pending_generations > 0
            || self.refining_streams > 0
    }
}
