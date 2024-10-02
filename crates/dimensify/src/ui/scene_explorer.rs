use bevy::prelude::*;
use bevy_editor_pls::{editor_window::EditorWindow, AddEditorWindow};
use bevy_egui::egui::{self, Ui};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use serde::{Deserialize, Serialize};

use crate::scene::node::Node;
use crate::scene_graphics::graphic_node::*;
use crate::{
    scene_graphics::graphic_node::{NodeWithGraphics, NodeWithGraphicsAndPhysics},
    GraphicsManager,
};

pub(super) fn plugin(app: &mut App) {
    app.add_editor_window::<SceneExplorerEditorWindow>();
}

pub(crate) struct SceneExplorerEditorWindow;

fn draw_scene_node(ui: &mut Ui, node: &mut NodeWithGraphicsAndPhysics, commands: &mut Commands) {
    if ui.button("Despawn").clicked() {
        node.despawn(commands);
    }
    ui.label(format!("{}", node));

    if let Some(children) = node.children_mut() {
        if !children.is_empty() {
            ui.collapsing("Children", |ui| {
                for mut child in children {
                    draw_scene_node(ui, child, commands);
                }
            });
        }
    }
}

// fn draw_scene_node<'a, T: 'a, U: 'a>(ui: &mut Ui, nodes: impl Iterator<Item = &'a Node<T, U>>) {
//     for (i, inner_node) in nodes.enumerate() {
//         ui.collapsing(format!("Node #{i}"), |ui| {
//             draw_scene_node(ui, inner_node.children().iter());
//         });
//     }
// }

impl EditorWindow for SceneExplorerEditorWindow {
    type State = RobotStateEditorState;
    const NAME: &'static str = "Scene Explorer";
    const DEFAULT_SIZE: (f32, f32) = (200., 150.);
    fn ui(
        world: &mut World,
        mut cx: bevy_editor_pls::editor_window::EditorWindowContext,
        ui: &mut egui::Ui,
    ) {
        let state = cx
            .state_mut::<SceneExplorerEditorWindow>()
            .expect("Failed to get robot window state");

        world.resource_scope(|world, mut graphics: Mut<GraphicsManager>| {
            let mut commands = world.commands();
            ui.heading(Self::NAME);
            for (i, obj) in graphics.scene.iter_object_mut().enumerate() {
                ui.collapsing(format!("Object #{i}"), |ui| {
                    for n in obj.iter_node_mut() {
                        draw_scene_node(ui, n, &mut commands);
                    }
                });
            }
        });

        // mut graphics: ResMut<GraphicsManager>,
        let graphics = world.resource::<GraphicsManager>();

        // ui.checkbox(&mut state.collider_render_enabled, "Colliders");
        // ui.checkbox(&mut state.navmesh_render_enabled, "Navmeshes");

        // ui.add(egui::Slider::new(&mut state.value, 0.0..=10.0).text("joint"));
        // if ui.button("+").clicked() {
        //     state.value += 1.0;
        // }
        // if ui.button("-").clicked() {
        //     state.value += 1.0;
        // }
    }

    fn app_setup(app: &mut App) {
        app.init_resource::<RobotStateEditorState>();
    }
}

#[derive(Debug, Clone, PartialEq, Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
#[derive(Default)]
pub(crate) struct RobotStateEditorState {
    pub(crate) collider_render_enabled: bool,
    pub(crate) navmesh_render_enabled: bool,
    value: f32,
}
