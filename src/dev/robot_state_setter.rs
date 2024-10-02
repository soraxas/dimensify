use std::time::Duration;
use std::{borrow::BorrowMut, ops::RangeInclusive};

use bevy::prelude::*;
use bevy::scene::ron::de;
use bevy_editor_pls::editor_window::EditorWindowContext;
use bevy_editor_pls::{editor_window::EditorWindow, AddEditorWindow};
use bevy_egui::egui::{self, CollapsingHeader, Slider};
use bevy_egui::EguiContext;
use egui::FontId;
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use rand::rngs::SmallRng;
use rand::{Rng, RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::robot_vis::{visuals::UrdfLoadRequest, RobotLinkMeshes, RobotState};

pub(super) fn plugin(app: &mut App) {
    app.register_type::<RobotShowColliderMesh>()
        .init_resource::<RobotShowColliderMesh>()
        .add_systems(Update, update_robot_link_meshes_visibilities)
        .add_editor_window::<RobotStateEditorWindow>();
}

pub(crate) struct EditorState {
    rng: SmallRng,
    robot_path: String,
}

impl EditorState {
    pub fn next_f32(&mut self) -> f32 {
        self.rng.next_u32() as f32 / u32::MAX as f32
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            rng: SmallRng::seed_from_u64(42),
            robot_path:
                "/home/soraxas/git-repos/robot-simulator-rs/assets/panda/urdf/panda_relative.urdf"
                    .to_string(),
        }
    }
}

pub(crate) struct RobotStateEditorWindow;

impl EditorWindow for RobotStateEditorWindow {
    type State = EditorState;

    const NAME: &'static str = "Robot Config";
    const DEFAULT_SIZE: (f32, f32) = (200., 150.);

    fn ui(world: &mut World, mut cx: EditorWindowContext, ui: &mut egui::Ui) {
        // TODO: look into file picker: https://github.com/kirjavascript/trueLMAO/blob/master/frontend/src/widgets/file.rs

        if let Some(editor_state) = &mut cx.state_mut::<Self>() {
            ui.text_edit_singleline(&mut editor_state.robot_path);
            if ui.button("load robot").clicked() {
                world.send_event(UrdfLoadRequest(editor_state.robot_path.clone()));
            }
        }

        for (mut state, entity) in world.query::<(&mut RobotState, Entity)>().iter_mut(world) {
            let mut changed = false;
            {
                let state = state.bypass_change_detection();

                CollapsingHeader::new(&state.urdf_robot.name)
                    .id_source(entity) // we use separate id sources to avoid conflicts
                    .default_open(true)
                    .show_background(true)
                    .show(ui, |ui| {
                        let randomise_joints = ui.button("Randomise joints").clicked();

                        let kinematic = &mut state.robot_chain;
                        for node in kinematic.iter() {
                            let mut new_pos = None;
                            // note that the following LOCK node, so we need to drop it before we can use it again (to set the position)

                            let joint_info = if let Some(parent) = node.mimic_parent() {
                                format!(" (mimic: {})", parent.joint().name)
                            } else {
                                "".to_string()
                            };
                            let joint = node.joint();

                            if let Some(cur_joint_position) = joint.joint_position() {
                                let mut joint_position = cur_joint_position;
                                let range = if let Some(limit) = joint.limits {
                                    RangeInclusive::new(limit.min, limit.max)
                                } else {
                                    // default to a full circle
                                    RangeInclusive::new(-std::f32::consts::PI, std::f32::consts::PI)
                                };

                                if randomise_joints {
                                    if let Some(editor_state) = &mut cx.state_mut::<Self>() {
                                        joint_position = range.start()
                                            + editor_state.next_f32()
                                                * (range.end() - range.start());
                                    }
                                }

                                ui.add(
                                    Slider::new(&mut joint_position, range)
                                        .suffix(" rad")
                                        .text(format!("{}{}", joint.name, joint_info)),
                                );

                                if joint_position != cur_joint_position {
                                    new_pos = Some(joint_position);
                                    changed = true;
                                }
                            } else {
                                ui.label(format!("> {} (fixed)", joint.name,));
                            }
                            // drop joint (which actually has a mutex lock on the node)
                            drop(joint);
                            if let Some(new_pos) = new_pos {
                                node.set_joint_position(new_pos)
                                    .expect("Front-end should prevent any out-of-range error");
                            }
                        }
                    });
            }
            if changed {
                state.set_changed();
            }
        }

        ui.separator();
        if let Some(mut collider_mesh_conf) = world.get_resource_mut::<RobotShowColliderMesh>() {
            ui.checkbox(&mut collider_mesh_conf.enabled, "Show collision meshes");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
#[derive(Default)]
pub(crate) struct RobotShowColliderMesh {
    pub(crate) enabled: bool,
}

fn update_robot_link_meshes_visibilities(
    conf: Res<RobotShowColliderMesh>,
    mut query: Query<(&RobotLinkMeshes, &mut Visibility)>,
) {
    if !conf.is_changed() {
        return;
    }

    let (desire_visual_mesh_visibility, desire_collider_mesh_visibility) = if conf.enabled {
        (Visibility::Hidden, Visibility::Visible)
    } else {
        (Visibility::Visible, Visibility::Hidden)
    };

    for (mesh, mut visible) in query.iter_mut() {
        match mesh {
            RobotLinkMeshes::Visual => {
                *visible = desire_visual_mesh_visibility;
            }
            RobotLinkMeshes::Collision => {
                *visible = desire_collider_mesh_visibility;
            }
        }
    }
}
