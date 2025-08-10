use bevy::prelude::*;

#[cfg(feature = "gsplat")]
pub mod gaussian_splatting;

pub mod preset;

#[allow(unused_variables)]
pub fn plugin(app: &mut App) {
    #[cfg(feature = "gsplat")]
    {
        app.add_plugins(gaussian_splatting::plugin);
    }
}
