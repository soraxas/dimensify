//! Neutral detail-selection and memory settings. Consumers supply product presets.
use bevy::prelude::*;

/// Selection policy, independent of the selected-cloud and memory ceilings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SogLodPolicy {
    /// Reference distance allocation.
    ReferenceDistance,
    /// Viewport pixel-density targeting with hysteresis.
    #[default]
    ScreenSpace,
}

/// Whether offscreen leaf ranges consume the selected-cloud budget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SogVisibilityPolicy {
    #[default]
    All,
    Frustum,
}

/// Requested per-scene detail; renderer/device memory limits apply afterward.
#[derive(Resource, Clone, Debug)]
pub struct SogLodSettings {
    pub lod_policy: SogLodPolicy,
    pub target_pixels: u32,
    pub splat_budget: usize,
    pub cache_bytes: usize,
    pub concurrent_chunks: usize,
    pub visibility: SogVisibilityPolicy,
}

impl SogLodSettings {
    /// Pixel-quality selection concerns the main view. Preserve the configured
    /// visibility for switching back to the reference distance allocator.
    pub fn effective_visibility(&self) -> SogVisibilityPolicy {
        match self.lod_policy {
            SogLodPolicy::ScreenSpace => SogVisibilityPolicy::Frustum,
            _ => self.visibility,
        }
    }
}

impl Default for SogLodSettings {
    fn default() -> Self {
        Self {
            lod_policy: default(),
            target_pixels: 2,
            splat_budget: 4_000_000,
            cache_bytes: if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
                96
            } else if cfg!(target_arch = "wasm32") {
                256
            } else {
                512
            } * 1024
                * 1024,
            concurrent_chunks: 2,
            visibility: SogVisibilityPolicy::All,
        }
    }
}
