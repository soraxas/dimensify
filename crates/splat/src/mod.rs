//! Optional Gaussian-splat loading and rendering for Dimensify.
#[cfg(feature = "render")]
use bevy::prelude::*;
#[cfg(feature = "render")]
use bevy_gaussian_splatting::PlanarGaussian3d;

/// A decoded cloud with its logical count and local-space center bounds.
#[cfg(feature = "render")]
pub struct DecodedSplat {
    /// Renderer data, including any alignment padding.
    pub cloud: PlanarGaussian3d,
    /// Number of authored Gaussians, excluding alignment padding.
    pub count: usize,
    /// Minimum and maximum Gaussian centers in source coordinates.
    pub bounds: (Vec3, Vec3),
}

#[cfg(feature = "render")]
mod decode;
#[cfg(feature = "render")]
pub use decode::decode_splat;

#[cfg(feature = "render")]
mod scene;
#[cfg(feature = "render")]
pub use scene::{
    LoadPhase, LoadSplat, LoadedSplat, SetSplatUp, SourceUp, SplatLoaded, SplatScene,
    SplatScenePlugin,
};

/// Install the Gaussian renderer and validated scene-loading systems.
/// Add this after Bevy's rendering and asset plugins.
#[cfg(feature = "render")]
pub struct SplatPlugin;
#[cfg(feature = "render")]
impl Plugin for SplatPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            bevy_gaussian_splatting::GaussianSplattingPlugin,
            SplatScenePlugin,
        ));
    }
}

/// Standalone native/browser viewer composition and local file controls.
#[cfg(feature = "viewer")]
pub mod viewer;

/// Streamed scene validation and view-dependent detail selection.
pub mod streaming;

/// Bounded resource resolution and HTTP transport.
#[cfg(feature = "streaming")]
pub mod assets;
/// Bounded streaming diagnostics.
#[cfg(feature = "streaming")]
pub mod diagnostics;
/// Renderer ownership and readiness adapters.
#[cfg(feature = "streaming")]
pub mod renderer;
/// Explicit renderer configuration.
#[cfg(feature = "streaming")]
pub mod runtime;
