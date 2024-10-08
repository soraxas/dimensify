use bevy::prelude::*;

pub(crate) mod main_camera;
pub(crate) mod window_camera;

pub fn plugin(app: &mut App) {
    app.add_plugins(window_camera::plugin)
        .add_plugins(main_camera::plugin);
}
