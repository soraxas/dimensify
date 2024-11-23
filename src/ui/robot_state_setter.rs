use std::ops::RangeInclusive;
use std::time::Duration;

use bevy::prelude::*;
use bevy_editor_pls::editor::EditorInternalState;
use bevy_editor_pls::editor_window::{open_floating_window, EditorWindowContext};
use bevy_editor_pls::{editor_window::EditorWindow, AddEditorWindow};
use bevy_egui::egui::{self, CollapsingHeader, Slider};
use egui::{Color32, DragValue, RichText};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use crate::robot::plugin::RobotLinkIsColliding;
use crate::robot_vis::show_colliding_link::{ConfCollidingContactPoints, ConfCollidingObjects};
use crate::robot_vis::visuals::UrdfLoadRequestParams;
use crate::util::traits::AsEguiDropdownExt;
use bevy_egui_notify::EguiToasts;
use rand::rngs::SmallRng;
use rand::{Rng, RngCore, SeedableRng};

use crate::robot_vis::display_options;
use crate::robot_vis::display_options::{ConfRobotLinkForceUseLinkMaterial, RobotDisplayMeshType};
use crate::robot_vis::{visuals::UrdfLoadRequest, RobotState};

pub(super) fn plugin(app: &mut App) {
    app.init_state::<RobotDisplayMeshType>()
        .init_state::<ConfRobotLinkForceUseLinkMaterial>()
        .add_systems(
            Update,
            display_options::update_robot_link_meshes_visibilities
                .run_if(on_event::<StateTransitionEvent<RobotDisplayMeshType>>()),
        )
        .add_systems(
            Update,
            display_options::update_robot_link_materials.run_if(on_event::<
                StateTransitionEvent<ConfRobotLinkForceUseLinkMaterial>,
            >()),
        )
        .add_systems(Startup, |mut writer: EventWriter<UrdfLoadRequest>| {
            writer.send(UrdfLoadRequest::new(
                // "/home/soraxas/git-repos/bullet3/examples/pybullet/gym/pybullet_data/r2d2.urdf"
                // "/home/soraxas/research/hap_pybullet/Push_env/Push_env/resources/ur5_shovel.urdf"
                "/home/soraxas/research/hap_pybullet/Push_env/Push_env/resources/ur5_shovel.urdf"
                    .to_string(),
                // "panda/urdf/panda_relative.urdf".to_string(),
                Some(
                    UrdfLoadRequestParams::default()
                        .fixed_base()
                        .with_collision_links(vec![
                            ("panda_hand".to_string(), "panda_link7".to_string()),
                            (
                                "panda_leftfinger".to_string(),
                                "panda_rightfinger".to_string(),
                            ),
                        ]),
                ),
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

    pub fn sample(&mut self, range: &RangeInclusive<f32>) -> f32 {
        range.start() + self.next_f32() * (*range.end() - *range.start())
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            rng: SmallRng::seed_from_u64(42),
            robot_path: "panda/urdf/panda_relative.urdf".to_string(),
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

    fn app_setup(app: &mut App) {
        app.add_systems(Startup, |internal_state: ResMut<EditorInternalState>| {
            open_floating_window::<Self>(internal_state.into_inner());
        });
    }

    fn ui(world: &mut World, mut cx: EditorWindowContext, ui: &mut egui::Ui) {
        // TODO: look into file picker: https://github.com/kirjavascript/trueLMAO/blob/master/frontend/src/widgets/file.rs

        let editor_state = &mut cx.state_mut::<Self>().unwrap();

        ui.text_edit_singleline(&mut editor_state.robot_path);
        if ui.button("load robot").clicked() {
            world.send_event(UrdfLoadRequest::from_file(editor_state.robot_path.clone()));
        }
        if ui.button("load robot ur5").clicked() {
            world.send_event(UrdfLoadRequest::from_file(
                "/home/soraxas/git-repos/hap_pybullet/Push_env/Push_env/resources/ur5_shovel.urdf"
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

                            let joint_info =  node.mimic_parent().map(|parent| format!("(mimic: {})", parent.joint().name));
                            let joint = node.joint();

                            if let Some(cur_joint_position) = joint.joint_position() {
                                let mut joint_position = cur_joint_position;

                                ui.horizontal(|ui| {
                                    ui.label(joint.name.clone());

                                    if let Some(limit) = joint.limits {
                                        let range = RangeInclusive::new(limit.min, limit.max);

                                        if randomise_joints {
                                            joint_position = editor_state.sample(&range);
                                        }

                                        ui.add(Slider::new(&mut joint_position, range));
                                    } else {
                                        // no joint limits
                                        if randomise_joints {
                                            const DEFAULT_RANGE: RangeInclusive<f32> = RangeInclusive::new(
                                                -1000.,
                                                1000.,
                                            );
                                            warn!("No joint limits for {}. Implicitly setting a limit of {} to {}",
                                                joint.name, DEFAULT_RANGE.start(), DEFAULT_RANGE.end());

                                            if randomise_joints {
                                                joint_position = editor_state.sample(&DEFAULT_RANGE);
                                            }
                                        }

                                        ui.add(DragValue::new(&mut joint_position).speed(0.1));
                                    }
                                    if let Some(joint_info) = joint_info {
                                        ui.label(joint_info);
                                    }
                                });

                                if joint_position != cur_joint_position {
                                    new_pos = Some(joint_position);
                                    changed = true;
                                }
                            } else {
                                ui.weak(format!("{} (fixed)", joint.name,));
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

        RobotDisplayMeshType::with_egui_dropdown(world, ui, "Display mesh type");

        // ConfRobotShowColliderMesh::with_bool(world, |val| {
        //     ui.checkbox(val, "Show collision meshes");
        // });

        ConfRobotLinkForceUseLinkMaterial::with_bool(world, |val| {
            ui.checkbox(val, "Force use link inline material tag");
        });

        ui.separator();

        ConfCollidingContactPoints::with_bool(world, |val| {
            ui.checkbox(val, "Show colliding contact points");
        });

        ConfCollidingObjects::with_bool(world, |val| {
            ui.checkbox(val, "Show colliding objects with colour");
        });

        // create an iterator that contains their name component, such that we can sort them

        let mut colliding_links: Vec<_> = world
            .query::<(Entity, &RobotLinkIsColliding)>()
            .iter(world)
            .flat_map(|(entity1, is_colliding)| {
                let name1 = world
                    .get::<Name>(entity1)
                    .map(|n| n.as_str())
                    .unwrap_or("Unknown");
                is_colliding
                    .entities
                    .iter()
                    .map(|entity2| {
                        let name2 = world
                            .get::<Name>(*entity2)
                            .map(|n| n.as_str())
                            .unwrap_or("Unknown");
                        (name1, name2)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        colliding_links.sort();

        for (entity1_name, entity2_name) in colliding_links {
            ui.horizontal(|ui| {
                ui.label("Colliding: ");
                ui.label(RichText::new(entity1_name).color(Color32::RED));
                ui.label(RichText::new(" <-> "));
                ui.label(RichText::new(entity2_name).color(Color32::RED));
            });
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
                                changed |=
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
