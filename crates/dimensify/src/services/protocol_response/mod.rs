use bevy::prelude::*;

pub(super) mod controller;
pub(super) mod draw;
pub(super) mod list;
pub(super) mod pending_response;

pub use controller::apply_new_commands;

pub fn plugin(app: &mut App) {
    app.init_resource::<controller::ViewerSettings>()
        .init_resource::<controller::ViewerState>()
        .init_resource::<controller::CommandCursor>()
        .add_plugins(controller::plugin)
        .add_plugins(draw::GizmoDrawPlugin);
}
