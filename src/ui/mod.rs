use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};
use bevy_editor_pls::prelude::*;
use bevy_egui_notify::EguiToastsPlugin;

pub(crate) mod rapier_debug_render;
pub(crate) mod robot_state_setter;
pub(crate) mod showcase_window;
pub(crate) mod spawn_window_camera;

/// Plugin with debugging utility intended for use during development only.
/// Don't include this in a release build.
pub fn plugin(app: &mut App) {
    app.add_plugins((
        FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin::filtered(vec![]),
        EguiToastsPlugin::default(),
        // bevy_rapier3d::render::RapierDebugRenderPlugin::default(),
    ))
    .add_plugins(bevy_egui::EguiPlugin);
    // .insert_gizmo_group(
    //     PhysicsGizmos {
    //         aabb_color: Some(Color::WHITE),
    //         ..default()
    //     },
    //     GizmoConfig {
    //         enabled: false,
    //         ..default()
    //     },
    // );
}

pub fn editor_pls_plugins(app: &mut App) {
    app.add_plugins(EditorPlugin::new())
        .insert_resource(default_editor_controls())
        .add_plugins((
            robot_state_setter::plugin,
            showcase_window::plugin,
            rapier_debug_render::plugin,
        ))
        .add_plugins(spawn_window_camera::plugin)
        .add_plugins(crate::camera::window_camera::plugin);
}

fn default_editor_controls() -> bevy_editor_pls::controls::EditorControls {
    use bevy_editor_pls::controls::*;
    let mut editor_controls = EditorControls::default_bindings();
    editor_controls.unbind(Action::PlayPauseEditor);
    editor_controls.insert(
        Action::PlayPauseEditor,
        Binding {
            input: UserInput::Single(Button::Keyboard(KeyCode::KeyQ)),
            conditions: vec![BindingCondition::ListeningForText(false)],
        },
    );
    editor_controls
}
