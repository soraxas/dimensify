use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};
use bevy_editor_pls::prelude::*;

pub(crate) mod dev_editor;
pub(crate) mod main_ui;
pub(crate) mod scene_explorer;
// pub(crate) mod rapier;
// pub(crate) mod robot_state_setter;

/// Plugin with debugging utility intended for use during development only.
/// Don't include this in a release build.
pub fn plugin(app: &mut App) {
    app.add_plugins(EditorPlugin::new())
        .insert_resource(default_editor_controls())
        .add_plugins((
            FrameTimeDiagnosticsPlugin,
            dev_editor::plugin,
            scene_explorer::plugin,
            // robot_state_setter::plugin,
            LogDiagnosticsPlugin::filtered(vec![]),
            // bevy_rapier3d::render::RapierDebugRenderPlugin::default(),
        ));
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
