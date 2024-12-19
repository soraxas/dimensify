use bevy::prelude::*;

#[cfg(feature = "gspat")]
pub mod gaussian_splatting;

pub mod preset;

#[allow(unused_variables)]
pub fn plugin(app: &mut App) {
    #[cfg(feature = "gspat")]
    {
        app.add(scene::gaussian_splatting::plugin);
    }
}
