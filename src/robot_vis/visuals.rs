use bevy::{app::App, ecs::system::EntityCommands};

use eyre::Result;
use rapier3d::prelude::ShapeType;
use std::f32::consts::*;
use thiserror::Error;

use bevy::prelude::*;
use urdf_rs::{Geometry, Pose};

use crate::{
    assets_loader::urdf::{UrdfAsset, UrdfLinkVisualComponents},
    graphics::prefab_assets::PrefabAssets,
};

use bevy_egui_notify::error_to_toast;

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
    link_entity: &mut EntityCommands,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    prefab_assets: &Res<PrefabAssets>,
    link_components: UrdfLinkVisualComponents,
    // mesh_material_key: &assets_loader::urdf::MeshMaterialMappingKey,
    // meshes_and_materials: &mut assets_loader::urdf::MeshMaterialMapping,
    element_container: VisualOrCollisionContainer,
) -> Entity {
    let origin_element = element_container.origin;

    let mut spatial_bundle = SpatialBundle::from_transform(Transform {
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
        ..Default::default()
    });
    if let Some(name) = element_container.name {
        link_entity.insert(Name::new(name.clone()));
    }

    let link_material = link_components.link_material.map(|m| {
        if m.material.is_none() {
            todo!("implements retrieving registered material from urdf root");
        }
        materials.add(m.material.unwrap())
    });

    match element_container.geometry {
        // if it is a mesh, they should have been pre-loaded.
        Geometry::Mesh { filename: _, scale } => {
            if let Some(val) = scale {
                spatial_bundle.transform.scale =
                    Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32);
            }
            link_entity.insert(spatial_bundle);

            link_entity.with_children(|builder| {
                let mut meshes_and_materials = link_components
                    .individual_meshes
                    .expect("if this is a mesh, it should have been pre-loaded");

                meshes_and_materials.drain(..).for_each(|(m, material)| {
                    let material_component = UrdfLinkMaterial {
                        // whole link material
                        from_inline_tag: link_material.clone(),
                        // individual mesh material
                        from_mesh_component: material.map(|m| materials.add(m)),
                    };

                    // if both are none, then we use the default material
                    let m_handle = match (
                        &material_component.from_inline_tag,
                        &material_component.from_mesh_component,
                    ) {
                        (_, Some(material)) => material.clone_weak(), // prortise material from mesh component
                        (Some(material), None) => material.clone_weak(),
                        (None, None) => prefab_assets.default_material.clone_weak(),
                    };

                    let bundle = PbrBundle {
                        mesh: meshes.add(m),
                        material: m_handle,
                        ..default()
                    };

                    builder.spawn(bundle).insert(material_component);
                });
            });
        }

        // let cube_h = meshes.add(Cuboid::new(0.1, 0.1, 0.1));
        // let sphere_h = meshes.add(Sphere::new(0.125).mesh().uv(32, 18));
        _ => (),
        Geometry::Box { size } => {
            let h = prefab_assets.get_prefab_mesh_handle(&ShapeType::Cuboid);
            link_entity.insert(spatial_bundle);

            // link_entity.insert(PbrBundle {
            //     mesh: h.clone_weak(),
            //     material: m_handle,
            //     ..default()
            // });

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
    link_entity.id()
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
    prefab_assets: Res<PrefabAssets>,
    mut urdf_assets: ResMut<Assets<UrdfAsset>>,
    mut reader: EventReader<UrdfAssetLoadedEvent>,
) {
    for event in reader.read() {
        if let Some(urdf_asset) = urdf_assets.remove(&event.0) {
            let mut urdf_robot = urdf_asset.robot;
            let mut meshes_and_materials = urdf_asset.meshes_and_materials;

            let mut robot_state = RobotState::new(urdf_robot.clone(), [].into());

            let mut robot_root = commands.spawn(RobotRoot);
            robot_root
                .insert(Name::new(urdf_robot.name))
                .insert(SpatialBundle::from_transform(Transform::from_rotation(
                    Quat::from_rotation_x(-FRAC_PI_2),
                )))
                .with_children(|child_builder: &mut ChildBuilder<'_>| {
                    for (i, mut link) in urdf_robot.links.drain(..).enumerate() {
                        let mut robot_link_entity = child_builder.spawn(RobotLink);

                        robot_state
                            .link_names_to_entity
                            .insert(link.name.clone(), robot_link_entity.id());

                        robot_link_entity
                            .insert(SpatialBundle::default())
                            .with_children(|child_builder| {
                                child_builder
                                    .spawn(RobotLinkMeshes::Visual)
                                    .insert(Name::new(format!("{}_visual", link.name)))
                                    .insert(SpatialBundle::default())
                                    .with_children(|child_builder| {
                                        for (j, visual) in link.visual.drain(..).enumerate() {
                                            let mesh_material_key =
                                                &(assets_loader::urdf::GeometryType::Visual, i, j);

                                            let link_components =meshes_and_materials.remove(mesh_material_key).expect("no mesh handles found, but it should have been pre-loaded"                                            );

                                            spawn_link(
                                                &mut child_builder.spawn_empty(),
                                                &mut materials,
                                                &mut meshes,
                                                &prefab_assets,
                            link_components,
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
                                    .insert(Name::new(format!("{}_collision", link.name)))
                                    .insert(SpatialBundle::HIDDEN_IDENTITY)
                                    .with_children(|child_builder| {
                                        for (j, collision) in link.collision.drain(..).enumerate() {
                                            let mesh_material_key = &(
                                                assets_loader::urdf::GeometryType::Collision,
                                                i,
                                                j,
                                            );
                                            let link_components =meshes_and_materials.remove(mesh_material_key).expect("no mesh handles found, but it should have been pre-loaded"                                            );

                                            spawn_link(
                                                &mut child_builder.spawn_empty(),
                                                &mut materials,
                                                &mut meshes,
                                                &prefab_assets,
link_components,
                                                VisualOrCollisionContainer {
                                                    name: &collision.name,
                                                    origin: &collision.origin,
                                                    geometry: &collision.geometry,
                                                    material: None,
                                                },
                                            );
                                        }
                                    });
                            })
                            .insert(Name::new(link.name));
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
