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
/// Opt-in whole-file input and synchronous preparation wall timings.
#[cfg(feature = "render")]
pub mod loading;
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

/// Opt-in progressive PLY network loading.
#[cfg(feature = "viewer")]
mod progressive;
#[cfg(all(test, feature = "render"))]
mod feature_tests {
    use super::*;

    fn renderer_app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Shader>();
        let mut render = bevy::app::SubApp::new();
        render.init_resource::<bevy::render::render_phase::DrawFunctions<
            bevy::core_pipeline::core_3d::Transparent3d,
        >>();
        app.insert_sub_app(bevy::render::RenderApp, render);
        app.add_plugins(SplatPlugin);
        app
    }

    #[test]
    fn cloud_asset_registration_matches_requested_dimensions() {
        let app = renderer_app();
        assert!(app.world().contains_resource::<Assets<PlanarGaussian3d>>());
        assert_eq!(
            app.world()
                .contains_resource::<Assets<bevy_gaussian_splatting::PlanarGaussian4d>>(),
            cfg!(feature = "gaussian-4d")
        );
    }

    #[test]
    fn loaders_match_requested_formats_and_keep_ply_and_sog() {
        let app = renderer_app();
        let assets = app.world().resource::<AssetServer>();
        for extension in ["ply", "sog"] {
            assert!(
                bevy::tasks::block_on(assets.get_asset_loader_with_extension(extension)).is_ok()
            );
        }
        for (extension, enabled) in [
            ("gltf", cfg!(feature = "gltf")),
            ("glb", cfg!(feature = "gltf")),
            ("ply4d", cfg!(feature = "gaussian-4d")),
            ("gc4d", cfg!(feature = "gaussian-4d")),
        ] {
            assert_eq!(
                bevy::tasks::block_on(assets.get_asset_loader_with_extension(extension)).is_ok(),
                enabled,
                "unexpected loader registration for {extension}"
            );
        }
    }
}
