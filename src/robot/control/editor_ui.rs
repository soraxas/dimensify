use bevy::prelude::*;
use bevy_editor_pls::{
    editor_window::{EditorWindow, EditorWindowContext},
    AddEditorWindow,
};

use crate::define_config_state;

use super::end_effector::{spawn_user_ee_marker, EndEffectorUserMarker};

pub(crate) fn plugin(app: &mut App) {
    app.init_state::<ConfEndeffectorControlMarker>()
        .add_editor_window::<RobotControlEditorWindow>()
        .add_systems(
            Update,
            spawn_or_despawn_marker
                .run_if(on_event::<StateTransitionEvent<ConfEndeffectorControlMarker>>),
        );
}

fn spawn_or_despawn_marker(
    conf: Res<State<ConfEndeffectorControlMarker>>,
    mut commands: Commands,
    q_markers: Query<Entity, With<EndEffectorUserMarker>>,
) {
    if conf.bool() {
        spawn_user_ee_marker(commands);
    } else {
        for entity in q_markers.iter() {
            commands.entity(entity).despawn();
        }
    }
}

define_config_state!(ConfEndeffectorControlMarker);

pub(crate) struct RobotControlEditorWindow;

impl EditorWindow for RobotControlEditorWindow {
    type State = ();

    const NAME: &'static str = "Robot ee Control";

    fn ui(world: &mut World, mut _cx: EditorWindowContext, ui: &mut egui::Ui) {
        ConfEndeffectorControlMarker::with_bool(world, |val| {
            ui.checkbox(val, "Use endeffector control");
        });
    }
}
