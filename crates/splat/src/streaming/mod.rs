//! Validated streamed SOG manifests and continuous view-dependent LOD policy.
//! These policies need neither renderer resources nor provider credentials.
mod distance;
mod manifest;
mod screen_space;

pub use distance::full_bounds_squared;
pub use manifest::{Bounds, Leaf, Manifest};
pub use screen_space::{PixelView, targets};

/// A contiguous logical range within a stable scene-local source file.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SplatRange {
    pub file: usize,
    pub offset: usize,
    pub count: usize,
}

mod settings;
pub use settings::{SogLodPolicy, SogLodSettings, SogVisibilityPolicy};

/// Progressive scene loading and render-ready selection ownership.
#[cfg(feature = "streaming")]
pub mod runtime;
