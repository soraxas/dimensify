use std::time::Duration;

#[cfg(feature = "physics")]
use crate::physics::PhysicsState;

#[cfg(feature = "physics")]
use crate::robot::urdf_loader::UrdfLoadRequest;

#[cfg(feature = "physics")]
use crate::robot::visual::show_colliding_link::{ConfCollidingContactPoints, ConfCollidingObjects};
use bevy::prelude::*;
use bevy_editor_pls::editor_window::EditorWindowContext;
use bevy_editor_pls::{editor_window::EditorWindow, AddEditorWindow};
use bevy_egui::egui::{self, CollapsingHeader};
use egui::{Color32, RichText};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use crate::robot::{RobotLinkIsColliding, RobotState};
// use crate::robot_vis::show_colliding_link::{ConfCollidingContactPoints, ConfCollidingObjects};
use crate::util::traits::AsEguiDropdownExt;
use bevy_egui_notify::EguiToasts;
use rand::rngs::SmallRng;
use rand::SeedableRng;

#[cfg(feature = "physics")]
use crate::robot::visual::display_options;

#[cfg(feature = "physics")]
use self::display_options::{ConfRobotLinkForceUseLinkMaterial, RobotDisplayMeshType};

use super::ui::ui_for_joint;

pub(crate) fn plugin(app: &mut App) {
    #[cfg(feature = "physics")]
    app.add_plugins(display_options::plugin);

    app.add_editor_window::<RobotStateEditorWindow>();
}

pub(crate) struct EditorState {
    rng: SmallRng,
    pub robot_path: String,
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

    fn ui(world: &mut World, mut cx: EditorWindowContext, ui: &mut egui::Ui) {
        // TODO: look into file picker: https://github.com/kirjavascript/trueLMAO/blob/master/frontend/src/widgets/file.rs

        let editor_state = &mut cx.state_mut::<Self>().unwrap();

        ui.text_edit_singleline(&mut editor_state.robot_path);

        #[cfg(feature = "physics")]
        if ui.button("load robot").clicked() {
            world.send_event(UrdfLoadRequest::from_file(editor_state.robot_path.clone()));
        }

        #[cfg(feature = "physics")]
        PhysicsState::with_egui_dropdown(world, ui, "Physics Engine");

        let mut maintance_request = None;
        for (mut state, entity) in world.query::<(&mut RobotState, Entity)>().iter_mut(world) {
            let mut changed = false;
            {
                let state = state.bypass_change_detection();

                CollapsingHeader::new(&state.urdf_robot.name)
                    .id_salt(entity) // we use separate id sources to avoid conflicts
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
                            let rng = if randomise_joints {
                                Some(&mut editor_state.rng)
                            } else {
                                None
                            };

                            let new_pos = ui_for_joint(ui, node, rng);
                            changed |= new_pos.is_some();
                            if let Some(new_pos) = new_pos {
                                match node.set_joint_position(new_pos) {
                                    Ok(_) => (),
                                    Err(e) => {
                                        error!(
                                            "Front-end should prevent any out-of-range error: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    });
            }
            if changed {
                state.set_changed();
            }
        }

        ui.separator();

        #[cfg(feature = "physics")]
        {
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
        }

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
                                    .get::<Mesh3d>(*child)
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
