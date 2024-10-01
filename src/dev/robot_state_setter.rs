use std::{borrow::BorrowMut, ops::RangeInclusive};

use bevy::prelude::*;
use bevy_editor_pls::{editor_window::EditorWindow, AddEditorWindow};
use bevy_egui::egui::{self, CollapsingHeader, Slider};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use serde::{Deserialize, Serialize};

use crate::robot_vis::{visuals::UrdfLoadRequest, RobotLinkMeshes, RobotState};

pub(super) fn plugin(app: &mut App) {
    app.register_type::<RobotShowColliderMesh>()
        .init_resource::<RobotShowColliderMesh>()
        .add_systems(Update, update_robot_link_meshes_visibilities)
        .add_editor_window::<RobotStateEditorWindow>();
}

pub(crate) struct RobotStateEditorWindow;

impl EditorWindow for RobotStateEditorWindow {
    type State = ();
    // type State = RobotShowColliderMesh;
    const NAME: &'static str = "Robot Config";
    const DEFAULT_SIZE: (f32, f32) = (200., 150.);
    fn ui(
        world: &mut World,
        _cx: bevy_editor_pls::editor_window::EditorWindowContext,
        ui: &mut egui::Ui,
    ) {
        if ui.button("load robot").clicked() {
            world.send_event(UrdfLoadRequest(
                "/home/soraxas/git-repos/robot-simulator-rs/assets/panda/urdf/panda_relative.urdf"
                    .to_owned(),
            ));
            // .add_systems(Startup, |mut writer: EventWriter<UrdfLoadRequest>| {
            //     writer.send(UrdfLoadRequest(
            //         "/home/soraxas/git-repos/robot-simulator-rs/assets/panda/urdf/panda_relative.urdf"
            //             .to_owned(),
            //     ));
            // })
        }

        for mut state in world.query::<&mut RobotState>().iter_mut(world) {
            let mut changed = false;
            {
                let state = state.bypass_change_detection();

                CollapsingHeader::new(&state.urdf_robot.name)
                    .default_open(true)
                    .show_background(true)
                    .show(ui, |ui| {
                        // ui.heading(&state.urdf_robot.name);
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

                            // ui.add(Slider::new(&mut joint.joint_position(), 1..=40).text(&joint.name).);

                            if let Some(cur_joint_position) = joint.joint_position() {
                                let mut joint_position = cur_joint_position;
                                let range = if let Some(limit) = joint.limits {
                                    RangeInclusive::new(limit.min, limit.max)
                                } else {
                                    // default to a full circle
                                    RangeInclusive::new(-std::f32::consts::PI, std::f32::consts::PI)
                                };

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
