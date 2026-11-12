//! Shared point-cloud loading and display, with an optional standalone viewer.

#[cfg(feature = "render")]
mod loading;
#[cfg(feature = "render")]
pub use loading::*;

#[cfg(feature = "render")]
mod files;
#[cfg(feature = "render")]
mod ply;
#[cfg(feature = "render")]
pub use files::{PointcloudFile, PotreeFiles};
#[cfg(feature = "render")]
pub use ply::decode_ply;

#[cfg(feature = "render")]
pub use bevy_pointcloud as renderer;

/// Optional native/browser viewer using the shared loading and camera APIs.
#[cfg(feature = "viewer")]
pub mod viewer;

#[cfg(test)]
#[cfg(feature = "render")]
mod tests;

#[cfg(all(test, feature = "render", not(target_arch = "wasm32")))]
mod render_tests;

// Temporary: exercise gallery output reuse in CI.
