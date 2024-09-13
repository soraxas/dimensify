use crate::util::error;
use bevy::prelude::*;
use bevy_editor_pls::{editor::Editor, editor_window::EditorWindow, AddEditorWindow};
use bevy_egui::egui;
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use serde::{Deserialize, Serialize};

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<RobotStateEditorState>()
        .add_editor_window::<RobotStateEditorWindow>()
       ;
}

pub(crate) struct RobotStateEditorWindow;

impl EditorWindow for RobotStateEditorWindow {
    type State = RobotStateEditorState;
    const NAME: &'static str = "Robot Config";
    const DEFAULT_SIZE: (f32, f32) = (200., 150.);
    fn ui(
        _world: &mut World,
        mut cx: bevy_editor_pls::editor_window::EditorWindowContext,
        ui: &mut egui::Ui,
    ) {
        let state = cx
            .state_mut::<RobotStateEditorWindow>()
            .expect("Failed to get robot window state");

        state.open = true;
        ui.heading("Debug Rendering");
        ui.checkbox(&mut state.collider_render_enabled, "Colliders");
        ui.checkbox(&mut state.navmesh_render_enabled, "Navmeshes");

        ui.add(egui::Slider::new(&mut state.value, 0.0..=10.0).text("joint"));
        if ui.button("+").clicked() {
            state.value += 1.0;
        }
        if ui.button("-").clicked() {
            state.value += 1.0;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
#[derive(Default)]
pub(crate) struct RobotStateEditorState {
    pub(crate) open: bool,
    pub(crate) collider_render_enabled: bool,
    pub(crate) navmesh_render_enabled: bool,
    value : f32,
}
