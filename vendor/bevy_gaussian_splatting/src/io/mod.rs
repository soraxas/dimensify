use bevy::prelude::*;

pub mod codec;
pub mod gcloud;
pub mod loader;
#[cfg(feature = "io_scene")]
pub mod scene;

#[cfg(feature = "io_ply")]
pub mod ply;

#[cfg(feature = "io_sog")]
pub mod sog;

#[cfg(feature = "io_sog")]
pub mod sog_packed;

#[derive(Default)]
pub struct IoPlugin;
impl Plugin for IoPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset_loader::<loader::Gaussian3dLoader>();
        #[cfg(feature = "gaussian_4d")]
        app.init_asset_loader::<loader::Gaussian4dLoader>();

        #[cfg(feature = "io_scene")]
        app.add_plugins(scene::GaussianScenePlugin);
    }
}
