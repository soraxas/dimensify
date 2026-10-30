//! Progressive streamed-SOG loading. HTTP and cache policy stay outside the renderer.

mod activation;
mod cache;
mod chunk;
pub mod diagnostics;
mod distance;
mod manifest;
pub mod overlay;
mod pages;
mod progress;
mod relocation;
mod replacement;
mod requests;
mod scheduling;
mod screen_space;
mod selection;
mod stream;
mod systems;
mod tick;
mod warm_cache;

use crate::assets::url_directory;
use crate::diagnostics::sog_cpu::{CpuStage, SogCpuTimings};
use crate::renderer::{self, SnapshotRenderer, SnapshotState};
use bevy::prelude::*;
use cache::Entry;
#[cfg(target_arch = "wasm32")]
pub mod codec;
use chunk::ChunkLoad;
use manifest::{Manifest, SplatRange};
use progress_types::{StreamProgress as ProgressTask, StreamProgressState as ProgressTaskState};
use renderer::source::{SourceStorage, packed_reservation};
mod progress_types;
pub use progress_types::{StreamProgress, StreamProgressState};
pub use systems::{SplatStreamingPlugin, install_sog_streaming, update_sog_streams};
#[cfg(test)]
use tick::next_files;

/// Camera used for streamed selection; consumers explicitly select their view.
#[derive(Component)]
pub struct StreamingCamera;
use std::collections::HashSet;

pub use crate::streaming::{SogLodSettings, SogVisibilityPolicy};

#[derive(Component)]
pub struct SogStream {
    manifest: Manifest,
    resolver: crate::assets::AssetResolver,
    base_url: String,
    pub(crate) name: String,
    pub progress_id: String,
    entries: Vec<Entry>,
    active: Vec<SplatRange>,
    /// Last successfully accepted selection, including a pending generation.
    submitted: Vec<SplatRange>,
    /// Render-ready generation, distinct from accepted or requested detail.
    displayed: Vec<SplatRange>,
    /// Lightweight debug evidence from the actual scheduler (no leaf text trace).
    overlay_intent: overlay::RefinementIntent,
    quality_history: Vec<usize>,
    quality_config: Option<(crate::streaming::SogLodPolicy, u32)>,
    submission_error: Option<String>,
    snapshot: SnapshotState,
    pending_work: bool,
    /// Active ranges and their CPU owners stay pinned during bounded uploads.
    preparing_upload: bool,
    next_update: f64,
    generation: u64,
    last_error: Option<String>,
    /// Task 6 opts in by setting Some; normal streaming allocates no leaf trace.
    pub diagnostics: Option<Vec<diagnostics::SogLeafDiagnostic>>,
    requested: usize,
    effective: usize,
    rejected_admission: Option<(Vec<SplatRange>, usize, Option<u64>)>,
    admission: selection::AdmissionSummary,
    /// Keep replacement intent through the covered-release/download stages.
    /// Otherwise resident old files can immediately win selection again.
    relocation_files: Option<Vec<bool>>,
    replacements: replacement::Replacements,
    /// Last observed live limits, not reserved permission to admit a source.
    replacement_limits: Option<(usize, usize, usize, Option<u64>)>,
    cpu_timings: SogCpuTimings,
    cache_trace: crate::diagnostics::sog_cache::SceneTrace,
    activation: activation::Tracker,
    storage: SourceStorage,
    pages: Option<pages::PageCache>,
    preparing_pages: bool,
}

/// Read-only capture/UI evidence; selection and transport remain stream-owned.
pub struct SogStreamStatus {
    pub evaluated: bool,
    pub requested: usize,
    pub effective: usize,
    pub selected: usize,
    pub displayed: usize,
    pub source_limited: bool,
    pub selection_limited: bool,
    pub pending_downloads: usize,
    pub pending_generation: bool,
    pub pending_work: bool,
    pub limiting_reason: String,
    pub cache_ready_bytes: u64,
    pub cache_reserved_bytes: u64,
}

#[cfg(test)]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod packed_tests;

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod camera_handoff_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod bounds_audit_tests;

#[cfg(all(any(test, feature = "test-support"), not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod raster_handoff_tests;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;
