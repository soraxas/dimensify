use bevy::app::App;

use eyre::Result;
use rapier3d::prelude::ShapeType;
use std::f32::consts::*;
use thiserror::Error;

use bevy::prelude::*;
use urdf_rs::{Geometry, Pose};

use crate::{
    assets_loader::urdf::UrdfAsset, dev::egui_toasts::error_to_toast,
    graphics::prefab_mesh::PrefabMesh,
};

use super::{
    assets_loader::{self},
    RobotLinkMeshes, RobotRoot,
};

// use super::assets_loader::{self, rgba_from_visual};

use crate::robot_vis::{RobotLink, RobotState};

#[derive(Error, Debug)]
pub enum UrdfAssetLoadingError {
    #[error("Failed to load urdf asset")]
    FailedToLoadUrdfAsset,
}

#[derive(Event, Debug, Default)]
pub struct UrdfLoadRequest(pub String);

#[derive(Debug, Clone, Eq, PartialEq, Resource, Default)]
pub struct PendingUrdlAsset(pub Vec<Handle<assets_loader::urdf::UrdfAsset>>);

#[derive(Event, Debug)]
pub struct UrdfAssetLoadedEvent(pub Handle<assets_loader::urdf::UrdfAsset>);

pub fn mesh_loader_plugin(app: &mut App) {
    app
        // .init_state::<UrdfLoadState>()
        .add_event::<UrdfLoadRequest>()
        .add_event::<UrdfAssetLoadedEvent>()
        .init_resource::<PendingUrdlAsset>()
        .add_plugins(assets_loader::urdf::plugin)
        // .add_systems(Startup, |mut writer: EventWriter<UrdfLoadRequest>| {
        //     writer.send(UrdfLoadRequest(
        //         "/home/soraxas/git-repos/robot-simulator-rs/assets/panda/urdf/panda_relative.urdf"
        //             .to_owned(),
        //     ));
        // })
        // handle incoming request to load urdf
        .add_systems(
            Update,
            load_urdf_request_handler.run_if(on_event::<UrdfLoadRequest>()),
        )
        // check the loading state
        .add_systems(
            Update,
            track_urdf_loading_state.pipe(error_to_toast).run_if(
                |pending_urdf_asset: Res<PendingUrdlAsset>| !pending_urdf_asset.0.is_empty(),
            ),
        )
        // process the loaded asset
        .add_systems(
            Update,
            load_urdf_meshes.run_if(on_event::<UrdfAssetLoadedEvent>()),
        );
}

/// request asset server to begin the load
fn load_urdf_request_handler(
    mut reader: EventReader<UrdfLoadRequest>,
    asset_server: Res<AssetServer>,
    mut pending_urdf_asset: ResMut<PendingUrdlAsset>,
) {
    for event in reader.read() {
        pending_urdf_asset
            .0
            .push(asset_server.load(event.0.clone()));
    }
}

fn track_urdf_loading_state(
    server: Res<AssetServer>,
    mut pending_urdf_asset: ResMut<PendingUrdlAsset>,
    mut writer: EventWriter<UrdfAssetLoadedEvent>,
) -> Result<()> {
    let original_length = pending_urdf_asset.0.len();
    {
        let pending_urdf_asset = pending_urdf_asset.bypass_change_detection();

        let mut tmp_vec = std::mem::take(&mut pending_urdf_asset.0);

        for handle in &mut tmp_vec.drain(..) {
            match server.get_load_states(handle.id()) {
                Some((_, _, bevy::asset::RecursiveDependencyLoadState::Loaded)) => {
                    writer.send(UrdfAssetLoadedEvent(handle));
                }
                Some((_, _, bevy::asset::RecursiveDependencyLoadState::Failed)) => {
                    return Err(UrdfAssetLoadingError::FailedToLoadUrdfAsset.into());
                }
                _ => pending_urdf_asset.0.push(handle),
            };
        }
    }
    if original_length != pending_urdf_asset.0.len() {
        // now triggers the changes
        pending_urdf_asset.set_changed();
    }
    Ok(())
}

/// This component should store the strong handle for each of these materials,
/// so that we can swap them
#[derive(Component, Debug, Default)]
pub struct UrdfLinkMaterial {
    /// a robot link can have an optional material tag
    pub from_inline_tag: Option<Handle<StandardMaterial>>,
    /// each link can have nested elements, which can have their own material
    pub from_mesh_component: Option<Handle<StandardMaterial>>,
}

fn spawn_link(
    entity: &mut bevy::ecs::system::EntityCommands,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    prefab_meshes: &Res<PrefabMesh>,
    mesh_material_key: &assets_loader::urdf::MeshMaterialMappingKey,
    standard_default_material: &mut Option<Handle<StandardMaterial>>,
    meshes_and_materials: &mut assets_loader::urdf::MeshMaterialMapping,
    element_container: VisualOrCollisionContainer,
    // geom_element: &Geometry,
    // origin_element: &Pose,
) -> Entity {
    let origin_element = element_container.origin;

    // let link_material = meshes_and_materials.

    // .is_some() {
    //                             dbg!(&element_container.material);
    //                             let m = element_container
    //                                 .material
    //                                 .clone()
    //                                 .unwrap()
    //                                 .color
    //                                 .clone()
    //                                 .unwrap()
    //                                 .clone();
    //                             bundle.material = materials.add(StandardMaterial {
    //                                 base_color: Color::srgba(
    //                                     m.rgba[0] as f32,
    //                                     m.rgba[1] as f32,
    //                                     m.rgba[2] as f32,
    //                                     m.rgba[3] as f32,
    //                                 ),
    //                                 ..Default::default()
    //                             });
    //                         }

    match element_container.geometry {
        Geometry::Mesh { filename: _, scale } => {
            let scale = scale.map_or_else(
                || Vec3::ONE,
                |val| Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32),
            );

            // dbg!(origin_element);
            // dbg!(&urdf_asset.meshes_and_materials);

            if element_container.material.is_some() {
                dbg!(&element_container.material);
                dbg!(&mesh_material_key);
                // panic!();
            }

            entity.insert(SpatialBundle::from_transform(Transform {
                translation: Vec3::new(
                    origin_element.xyz[0] as f32,
                    origin_element.xyz[1] as f32,
                    origin_element.xyz[2] as f32,
                ),
                rotation: Quat::from_euler(
                    EulerRot::XYZ,
                    origin_element.rpy[0] as f32,
                    origin_element.rpy[1] as f32,
                    origin_element.rpy[2] as f32,
                ),
                scale,
            }));
            if let Some(name) = element_container.name {
                entity.insert(Name::new(name.clone()));
            }
            entity.with_children(|builder| {
                match meshes_and_materials.remove(mesh_material_key) {
                    None => {
                        error!(
                            "no mesh handles found for {:?}. But it should have been pre-loaded",
                            mesh_material_key
                        );
                    }
                    Some(mut meshes_and_materials) => {
                        let link_material =
                            meshes_and_materials.link_material.map(|m| materials.add(m));

                        meshes_and_materials.individual_meshes.drain(..).for_each(
                            |(m, material)| {
                                let mut bundle = PbrBundle {
                                    mesh: meshes.add(m),
                                    ..default()
                                };

                                let mut material_component = UrdfLinkMaterial::default();
                                // whole link material
                                if link_material.is_some() {
                                    material_component.from_inline_tag = link_material.clone();
                                }
                                // individual mesh material
                                if let Some(material) = material {
                                    material_component.from_mesh_component =
                                        Some(materials.add(material));
                                }

                                // if both are none, then we use the default material
                                let m_handle = match (
                                    &material_component.from_inline_tag,
                                    &material_component.from_mesh_component,
                                ) {
                                    (_, Some(material)) => material.clone_weak(), // prortise material from mesh component
                                    (Some(material), None) => material.clone_weak(),
                                    (None, None) => {
                                        if standard_default_material.is_none() {
                                            // create standard material on demand
                                            *standard_default_material = Some(
                                                materials.add(StandardMaterial { ..default() }),
                                            );
                                        }
                                        standard_default_material.as_ref().unwrap().clone()
                                    }
                                };

                                bundle.material = m_handle;

                                // bundle.material = match material {
                                //     Some(material) => materials.add(material),
                                //     None => {
                                //         if standard_default_material.is_none() {
                                //             // create standard material on demand
                                //             *standard_default_material =
                                //                 Some(materials.add(StandardMaterial { ..default() }));
                                //         }
                                //         standard_default_material.as_ref().unwrap().clone()
                                //         // unwrap cannot fails as we've just added it
                                //     }
                                // };

                                builder.spawn(bundle).insert(material_component);
                            },
                        );
                    }
                }
            });
        }

        // let cube_h = meshes.add(Cuboid::new(0.1, 0.1, 0.1));
        // let sphere_h = meshes.add(Sphere::new(0.125).mesh().uv(32, 18));
        _ => (),
        Geometry::Box { size } => {
            let h = prefab_meshes.get_prefab_mesh_handle(&ShapeType::Cuboid);

            // entity
            //     .insert(SpatialBundle::from_transform(
            //              Transform {
            //                 translation: Vec3::new(
            //                     origin_element.xyz[0] as f32,
            //                     origin_element.xyz[1] as f32,
            //                     origin_element.xyz[2] as f32,
            //                 ),
            //                 rotation: Quat::from_euler(
            //                     EulerRot::XYZ,
            //                     origin_element.rpy[0] as f32,
            //                     origin_element.rpy[1] as f32,
            //                     origin_element.rpy[2] as f32,
            //                 ),
            //                 scale: Vec3::ONE,
            //             },
            //     ))
            //     .with_children(|builder| {

            //                 let mut bundle = PbrBundle {
            //                     mesh: h.clone(),
            //                     ..default()
            //                 };
            //                 bundle.material = match material {
            //                     Some(material) => materials.add(material),
            //                     None => {
            //                         if standard_default_material.is_none() {
            //                             // create standard material on demand
            //                             *standard_default_material =
            //                                 Some(materials.add(StandardMaterial { ..default() }));
            //                         }
            //                         standard_default_material.as_ref().unwrap().clone()  // unwrap cannot fails as we've just added it
            //                     }
            //                 };

            //                 builder.spawn(bundle);

            //         match meshes_and_materials.remove(mesh_material_key) {
            //         None => { error!("no mesh handles found for {:?}. But it should have been pre-loaded", mesh_material_key); }
            //         Some(mut meshes_and_materials) => {
            //             meshes_and_materials.drain(..).for_each(|(m, material)| {
            //                 let mut bundle = PbrBundle {
            //                     mesh: meshes.add(m),
            //                     ..default()
            //                 };
            //                 bundle.material = match material {
            //                     Some(material) => materials.add(material),
            //                     None => {
            //                         if standard_default_material.is_none() {
            //                             // create standard material on demand
            //                             *standard_default_material =
            //                                 Some(materials.add(StandardMaterial { ..default() }));
            //                         }
            //                         standard_default_material.as_ref().unwrap().clone()  // unwrap cannot fails as we've just added it
            //                     }
            //                 };

            //                 builder.spawn(bundle);
            //             });
            //         }
            //     }
            //     });
        } // Geometry::Cylinder { radius, length } => todo!(),
          // Geometry::Capsule { radius, length } => todo!(),
          // Geometry::Sphere { radius } => todo!(),
          // Geometry::Mesh { filename, scale } => todo!(),
    }
    entity.id()
}

struct VisualOrCollisionContainer<'a> {
    pub name: &'a Option<String>,
    pub origin: &'a Pose,
    pub geometry: &'a Geometry,
    pub material: Option<&'a urdf_rs::Material>,
}

/// this gets triggers on event UrdfAssetLoadedEvent (which checks that handles are loaded)
fn load_urdf_meshes(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    prefab_meshes: Res<PrefabMesh>,
    mut urdf_assets: ResMut<Assets<UrdfAsset>>,
    mut reader: EventReader<UrdfAssetLoadedEvent>,
) {
    for event in reader.read() {
        let handle = &event.0;

        if let Some(urdf_asset) = urdf_assets.remove(handle) {
            let urdf_robot = urdf_asset.robot;
            let mut meshes_and_materials = urdf_asset.meshes_and_materials;

            let mut robot_state = RobotState::new(urdf_robot.clone(), [].into());

            let mut standard_default_material = None;

            let mut robot_root = commands.spawn(RobotRoot);
            robot_root
                .insert(Name::new(urdf_robot.name))
                .insert(SpatialBundle::from_transform(Transform::from_rotation(
                    Quat::from_rotation_x(-FRAC_PI_2),
                )))
                .with_children(|child_builder| {
                    for (i, l) in urdf_robot.links.iter().enumerate() {
                        let mut robot_link_entity = child_builder.spawn(RobotLink);

                        robot_state
                            .link_names_to_entity
                            .insert(l.name.clone(), robot_link_entity.id());

                        robot_link_entity
                            .insert(SpatialBundle::default())
                            .insert(Name::new(l.name.clone()))
                            .with_children(|child_builder| {
                                child_builder
                                    .spawn(RobotLinkMeshes::Visual)
                                    .insert(Name::new(format!("{}_visual", l.name)))
                                    .insert(SpatialBundle::default())
                                    .with_children(|child_builder| {
                                        for (j, visual) in l.visual.iter().enumerate() {
                                            let mesh_material_key =
                                                &(assets_loader::urdf::MeshType::Visual, i, j);
                                            spawn_link(
                                                &mut child_builder.spawn_empty(),
                                                &mut materials,
                                                &mut meshes,
                                                &prefab_meshes,
                                                mesh_material_key,
                                                &mut standard_default_material,
                                                &mut meshes_and_materials,
                                                VisualOrCollisionContainer {
                                                    name: &visual.name,
                                                    origin: &visual.origin,
                                                    geometry: &visual.geometry,
                                                    material: visual.material.as_ref(),
                                                },
                                            );
                                        }
                                    });

                                child_builder
                                    .spawn(RobotLinkMeshes::Collision)
                                    .insert(Name::new(format!("{}_collision", l.name)))
                                    .insert(SpatialBundle::HIDDEN_IDENTITY)
                                    .with_children(|child_builder| {
                                        for (j, collision) in l.collision.iter().enumerate() {
                                            let mesh_material_key =
                                                &(assets_loader::urdf::MeshType::Collision, i, j);
                                            spawn_link(
                                                &mut child_builder.spawn_empty(),
                                                &mut materials,
                                                &mut meshes,
                                                &prefab_meshes,
                                                mesh_material_key,
                                                &mut standard_default_material,
                                                &mut meshes_and_materials,
                                                VisualOrCollisionContainer {
                                                    name: &collision.name,
                                                    origin: &collision.origin,
                                                    geometry: &collision.geometry,
                                                    material: None,
                                                },
                                            );
                                        }
                                    });
                            });
                    }
                });
            robot_root.insert(robot_state);
            // commands.insert_resource(robot_state);
        } else {
            error!("Failed to load urdf asset, even though it's loaded");
        };
    }
}

#[derive(Bundle, Default)]
pub struct RobotLinkBundle {
    pub spatial: SpatialBundle,
    _link: RobotLink,
}
