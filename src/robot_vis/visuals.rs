use crate::util::coordinate_transform::CoordinateSysTransformToBevy;
use bevy::{app::App, ecs::system::EntityCommands, utils::hashbrown::HashMap};

use eyre::Result;
use rapier3d::prelude::ShapeType;
use std::f32::consts::*;
use thiserror::Error;

use bevy::prelude::*;
use urdf_rs::{Geometry, Pose};

use crate::{
    assets_loader::urdf::{UrdfAsset, UrdfLinkComponents},
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

/// one robot link can have multiple visual or collision elements. This spawns
/// a unit of element
fn spawn_link_component(
    link_entity: &mut EntityCommands,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    robot_materials_registry: &mut HashMap<String, Handle<StandardMaterial>>,
    prefab_assets: &Res<PrefabAssets>,
    link_components: UrdfLinkComponents,
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
            // try to retrieve from shared registry
            dbg!(&robot_materials_registry);
            dbg!(&m);
            robot_materials_registry
                .get(&m.name)
                .expect("material not found in robot's materials registry")
                .clone()
        } else {
            let handle = materials.add(m.material.unwrap());
            robot_materials_registry.insert(m.name.clone(), handle.clone_weak());
            handle
        }
    });

    link_entity.with_children(|child_builder| {
        match element_container.geometry {
            // if it is a mesh, they should have been pre-loaded.
            Geometry::Mesh { filename: _, scale } => {
                if let Some(val) = scale {
                    spatial_bundle.transform.scale =
                        Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32);
                }

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
                    child_builder.spawn(bundle).insert(material_component);
                });
            }

            // let cube_h = meshes.add(Cuboid::new(0.1, 0.1, 0.1));
            // let sphere_h = meshes.add(Sphere::new(0.125).mesh().uv(32, 18));
            primitive_geometry => {
                let handle = match prefab_assets
                    .get_prefab_mesh_handle_and_scale_from_urdf_geom(primitive_geometry)
                {
                    Some((scale, prefab_handle)) => {
                        spatial_bundle.transform.scale = scale;
                        prefab_handle.clone_weak()
                    }
                    None => match primitive_geometry {
                        Geometry::Capsule { radius, length } => {
                            use bevy::prelude::Capsule3d;
                            let shape = Capsule3d {
                                radius: *radius as f32,
                                half_length: (length / 2.) as f32,
                            };
                            meshes.add(shape)
                        }
                        _ => unreachable!(),
                    },
                };

                // NOTE: if it is a primitive, we NEED to rotate it by 90 degrees, as
                // urdf uses z-axis as the up axis, while bevy uses y-axis as the up axis
                spatial_bundle.transform.to_bevy_inplace();

                child_builder.spawn(PbrBundle {
                    mesh: handle,
                    material: link_material
                        .unwrap_or_else(|| prefab_assets.default_material.clone_weak()),
                    ..default()
                });
            }
        }
    });
    link_entity.insert(spatial_bundle);

    link_entity.id()
}

struct VisualOrCollisionContainer<'a> {
    pub name: &'a Option<String>,
    pub origin: &'a Pose,
    pub geometry: &'a Geometry,
    // pub material: Option<&'a urdf_rs::Material>,
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
        if let Some(mut urdf_asset) = urdf_assets.remove(&event.0) {
            let mut urdf_robot = urdf_asset.robot;
            let mut meshes_and_materials = urdf_asset.link_meshes_materials;

            let mut robot_state = RobotState::new(urdf_robot.clone(), [].into());

            let mut robot_materials_registry = urdf_asset
                .root_materials
                .drain()
                .map(|(name, material)| (name, materials.add(material)))
                .collect::<HashMap<_, _>>();

            let mut robot_root = commands.spawn(RobotRoot);
            robot_root
                .insert(Name::new(urdf_robot.name))
                .insert(SpatialBundle::from_transform(
                    Transform::default().to_bevy()
            ))
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

                                            spawn_link_component(
                                                &mut child_builder.spawn_empty(),
                                                &mut materials,
                                                &mut meshes,
                                                &mut robot_materials_registry,
                                                &prefab_assets,
                                                link_components,
                                                VisualOrCollisionContainer {
                                                    name: &visual.name,
                                                    origin: &visual.origin,
                                                    geometry: &visual.geometry,
                                                    // material: visual.material.as_ref(),
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

                                            spawn_link_component(
                                                &mut child_builder.spawn_empty(),
                                                &mut materials,
                                                &mut meshes,
                                                &mut robot_materials_registry,
                                                &prefab_assets,
                                                link_components,
                                                VisualOrCollisionContainer {
                                                    name: &collision.name,
                                                    origin: &collision.origin,
                                                    geometry: &collision.geometry,
                                                    // material: None,
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
