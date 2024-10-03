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

use crate::assets_loader::mesh;
use bevy_egui_notify::EguiToasts;

use crate::robot_vis::visuals::UrdfLinkMaterial;
use crate::robot_vis::RobotRoot;
use crate::robot_vis::{visuals::UrdfLoadRequest, RobotLinkMeshes, RobotState};

pub(super) fn plugin(app: &mut App) {
    app.register_type::<RobotShowColliderMesh>()
        .init_resource::<RobotShowColliderMesh>()
        .register_type::<RobotLinkForceUseLinkMaterial>()
        .init_resource::<RobotLinkForceUseLinkMaterial>()
        .add_systems(Update, update_robot_link_meshes_visibilities)
        .add_systems(Update, update_robot_link_materials)
        .add_systems(Startup, |mut writer: EventWriter<UrdfLoadRequest>| {
            writer.send(UrdfLoadRequest(
                "/home/soraxas/research/hap_pybullet/Push_env/Push_env/resources/ur5_shovel.urdf"
                    .to_string(),
            ));
        })
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

enum RobotMaintanceRequest {
    ComputeFlatNormal,
}

pub(crate) struct RobotStateEditorWindow;

impl EditorWindow for RobotStateEditorWindow {
    type State = EditorState;

    const NAME: &'static str = "Robot Config";
    const DEFAULT_SIZE: (f32, f32) = (200., 150.);

    fn ui(world: &mut World, mut cx: EditorWindowContext, ui: &mut egui::Ui) {
        // TODO: look into file picker: https://github.com/kirjavascript/trueLMAO/blob/master/frontend/src/widgets/file.rs

        let editor_state = &mut cx.state_mut::<Self>().unwrap();

        ui.text_edit_singleline(&mut editor_state.robot_path);
        if ui.button("load robot").clicked() {
            world.send_event(UrdfLoadRequest(editor_state.robot_path.clone()));
        }
        if ui.button("load robot ur5").clicked() {
            world.send_event(UrdfLoadRequest(
                "/home/soraxas/research/hap_pybullet/Push_env/Push_env/resources/ur5_shovel.urdf"
                    .to_string(),
            ));
        }

        let mut maintance_request = None;
        for (mut state, entity) in world.query::<(&mut RobotState, Entity)>().iter_mut(world) {
            let mut changed = false;
            {
                ////////////

                let state = state.bypass_change_detection();

                CollapsingHeader::new(&state.urdf_robot.name)
                    .id_source(entity) // we use separate id sources to avoid conflicts
                    .default_open(true)
                    .show_background(true)
                    .show(ui, |ui| {
                        let randomise_joints = ui.button("Randomise joints").clicked();

                        // mesh maintance
                        if ui.button("Recompute mesh normals").clicked() {
                            maintance_request =
                                Some((entity, RobotMaintanceRequest::ComputeFlatNormal));
                        }

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
                                    joint_position = range.start()
                                        + editor_state.next_f32() * (range.end() - range.start());
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

        if let Some(mut collider_mesh_conf) =
            world.get_resource_mut::<RobotLinkForceUseLinkMaterial>()
        {
            ui.checkbox(
                &mut collider_mesh_conf.enabled,
                "Force use link inline material tag",
            );
        }

        // perform actions
        if let Some((entity, action)) = maintance_request {
            match action {
                RobotMaintanceRequest::ComputeFlatNormal => {
                    /// a helper function that recursively computes normals for all descendants of an entity
                    /// with a mesh handle
                    fn compute_normals_recursive<'a>(
                        world: &'a World,
                        entity: Entity,
                        meshes: &mut Assets<Mesh>,
                        toasts: &mut EguiToasts,
                        mut name: Option<&'a Name>,
                    ) -> bool {
                        let mut changed = false;
                        // extract the link name
                        let inner_name = world.get::<Name>(entity);
                        if inner_name.is_some() {
                            name = inner_name;
                        }

                        if let Some(children) = world.get::<Children>(entity) {
                            for child in children {
                                if let Some(mesh) = world
                                    .get::<Handle<Mesh>>(*child)
                                    .and_then(|mesh_handle| meshes.get_mut(mesh_handle))
                                {
                                    if mesh.contains_attribute(Mesh::ATTRIBUTE_NORMAL) {
                                        // these are meshes that has no normals (indices)
                                        mesh.compute_normals();
                                        toasts
                                            .0
                                            .info(format!(
                                                "Computed flat normals for {}.",
                                                name.map(|n| n.as_str()).unwrap_or("some entity")
                                            ))
                                            .duration(Some(Duration::from_secs(8)));
                                        changed = true;
                                    }
                                }
                                // go to descendants
                                compute_normals_recursive(world, *child, meshes, toasts, name);
                            }
                        }
                        changed
                    }

                    world.resource_scope(|world, meshes: Mut<Assets<Mesh>>| {
                        world.resource_scope(|world, toasts: Mut<EguiToasts>| {
                            let toasts = toasts.into_inner();
                            if !compute_normals_recursive(
                                world,
                                entity,
                                meshes.into_inner(),
                                toasts,
                                None,
                            ) {
                                toasts
                                    .0
                                    .info("No computations happened.")
                                    .duration(Some(Duration::from_secs(8)));
                            }
                        });
                    });
                }
            }
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

#[derive(Debug, Clone, PartialEq, Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
#[derive(Default)]
pub(crate) struct RobotLinkForceUseLinkMaterial {
    pub(crate) enabled: bool,
}

fn update_robot_link_materials(
    conf: Res<RobotLinkForceUseLinkMaterial>,
    mut query: Query<(&UrdfLinkMaterial, &mut Handle<StandardMaterial>)>,
) {
    if !conf.is_changed() {
        return;
    }

    for (link_material_container, mut handle) in query.iter_mut() {
        match (
            conf.enabled,
            &link_material_container.from_inline_tag,
            &link_material_container.from_mesh_component,
        ) {
            (true, Some(inline_material), _) => {
                *handle = inline_material.clone_weak();
            }
            (_, _, Some(mesh_material)) => {
                *handle = mesh_material.clone_weak();
            }
            (_, Some(inline_material), _) => {
                *handle = inline_material.clone_weak();
            }
            (_, None, None) => {
                // do nothing
            }
        }
    }
}
