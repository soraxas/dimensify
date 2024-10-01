use bevy::{app::App, asset::LoadState};

use std::f32::consts::*;

use bevy::prelude::*;
use urdf_rs::{Geometry, Pose};

use crate::assets_loader::urdf::UrdfAsset;

use super::{
    assets_loader::{self},
    RobotLinkMeshes, RobotRoot,
};

// use super::assets_loader::{self, rgba_from_visual};

use crate::robot_vis::{RobotLink, RobotState};

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
            track_urdf_loading_state.run_if(|pending_urdf_asset: Res<PendingUrdlAsset>| {
                !pending_urdf_asset.0.is_empty()
            }),
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
) {
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
                    error!("Failed to load urdf asset");
                }
                _ => pending_urdf_asset.0.push(handle),
            };
        }
    }
    if original_length != pending_urdf_asset.0.len() {
        // now triggers the changes
        pending_urdf_asset.set_changed();
    }
}

fn spawn_link(
    entity: &mut bevy::ecs::system::EntityCommands,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    mesh_material_key: &assets_loader::urdf::MeshMaterialMappingKey,
    standard_default_material: &mut Option<Handle<StandardMaterial>>,
    meshes_and_materials: &mut assets_loader::urdf::MeshMaterialMapping,
    geom_element: &Geometry,
    origin_element: &Pose,
) -> Entity {
    match *geom_element {
        urdf_rs::Geometry::Mesh { filename: _, scale } => {
            let scale = scale.map_or_else(
                || Vec3::ONE,
                |val| Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32),
            );

            // dbg!(origin_element);
            // dbg!(&urdf_asset.meshes_and_materials);

            entity
                .insert(SpatialBundle::from_transform(
                         Transform {
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
                            scale: scale,
                        },
                ))
                .with_children(|builder| {
                    match meshes_and_materials.remove(mesh_material_key) {
                    None => { error!("no mesh handles found for {:?}. But it should have been pre-loaded", mesh_material_key); }
                    Some(mut meshes_and_materials) => {
                        meshes_and_materials.drain(..).for_each(|(m, material)| {
                            let mut bundle = PbrBundle {
                                mesh: meshes.add(m),
                                ..default()
                            };
                            bundle.material = match material {
                                Some(material) => materials.add(material),
                                None => {
                                    if standard_default_material.is_none() {
                                        // create standard material on demand
                                        *standard_default_material =
                                            Some(materials.add(StandardMaterial { ..default() }));
                                    }
                                    standard_default_material.as_ref().unwrap().clone()  // unwrap cannot fails as we've just added it
                                }
                            };

                            builder.spawn(bundle);
                        });
                    }
                }
                });
        }
        _ => {
            todo!();
        }
    }
    entity.id()
}

/// this gets triggers on event UrdfAssetLoadedEvent (which checks that handles are loaded)
fn load_urdf_meshes(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
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
                                                mesh_material_key,
                                                &mut standard_default_material,
                                                &mut meshes_and_materials,
                                                &visual.geometry,
                                                &visual.origin,
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
                                                mesh_material_key,
                                                &mut standard_default_material,
                                                &mut meshes_and_materials,
                                                &collision.geometry,
                                                &collision.origin,
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
