use bevy::prelude::*;

pub(crate) mod floating_cam_editor_ui;
pub mod main_camera;
pub mod window_camera;

pub fn plugin(app: &mut App) {
    app.add_plugins(main_camera::plugin)
        .add_plugins(window_camera::plugin);
}
