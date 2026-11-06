use crate::{
    coordinate_system::prelude::*,
    robot::{RobotLink, RobotState},
};
use bevy::{
    app::App,
    asset::AssetLoadError,
    ecs::{relationship::RelatedSpawnerCommands, system::EntityCommands},
    platform::collections::HashMap,
};

// use eyre::Result;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use bevy::prelude::*;
use urdf_rs::{Geometry, Pose};

use crate::urdf_assets_loader as assets_loader;

use assets_loader::urdf::{UrdfAsset, UrdfLinkComponents};

// #[cfg(feature = "physics")]
use crate::graphics::prefab_assets::PrefabAssets;

// use bevy_egui_notify::{EguiToasts, error_to_toast};

use super::{RobotLinkMeshesType, RobotRoot, control::end_effector::EndEffectorTarget};

// use super::assets_loader::{self, rgba_from_visual};

#[derive(Debug, Default, Component, Clone)]
pub struct RobotLinkInitOptions(pub Vec<RobotLinkInitOption>);

impl From<Vec<RobotLinkInitOption>> for RobotLinkInitOptions {
    fn from(val: Vec<RobotLinkInitOption>) -> Self {
        Self(val)
    }
}

#[derive(Debug, Clone)]
pub enum RobotLinkInitOption {
    AsEndEffectorTarget(EndEffectorTarget),
    WithAttachedCamera {
        camera_origin: Transform,
        image_width: u32,
        image_height: u32,
    },
}

impl From<EndEffectorTarget> for RobotLinkInitOption {
    fn from(val: EndEffectorTarget) -> Self {
        Self::AsEndEffectorTarget(val)
    }
}

#[derive(Error, Debug)]
pub enum UrdfAssetLoadingError {
    #[error("Failed to retrieve urdf asset, even though it should have been loaded")]
    MissingUrdfAsset,
    #[error("Failed to load urdf asset: {0}")]
    FailedToLoadUrdfAsset(Arc<AssetLoadError>),
    #[error("The given link in ignored link-pair does not exists: {0}")]
    InvalidLinkPairToIgnore(String),
}

#[derive(Debug, Default)]
pub struct UrdfLoadRequestParams {
    pub ignored_linkpair_collision: Vec<(String, String)>,
    pub transform: Transform,
    pub fixed_base: bool,
    pub initial_joint_values: HashMap<String, f32>,
    pub joint_init_options: HashMap<String, RobotLinkInitOptions>,
}

impl UrdfLoadRequestParams {
    /// This is a helper function to create a default fixed base.
    /// The root link will be fixed to the world and without collision to floor.
    pub fn fixed_base(mut self) -> Self {
        self.fixed_base = true;
        self
    }

    pub fn with_collision_links(mut self, links: Vec<(String, String)>) -> Self {
        self.ignored_linkpair_collision = links;
        self
    }
}

#[derive(Message, Debug, Default)]
pub struct UrdfLoadRequest {
    /// file to load
    pub filename: String,
    /// pairs of links that are allowed to collide (e.g. links that are next to each other)
    /// but we know that they should not collide, by design.
    pub params: Arc<Mutex<UrdfLoadRequestParams>>,
}

impl UrdfLoadRequest {
    pub fn new(filename: String, params: Option<UrdfLoadRequestParams>) -> Self {
        Self {
            filename,
            params: Arc::new(params.unwrap_or_default().into()),
        }
    }

    pub fn from_file(filename: String) -> Self {
        Self::new(filename, None)
    }

    pub fn with_params(mut self, params: UrdfLoadRequestParams) -> Self {
        self.params = Arc::new(params.into());
        self
    }
}

#[derive(Debug, Clone, Resource, Default)]
pub struct PendingUrdfAsset(
    pub(crate)  Vec<(
        Handle<assets_loader::urdf::UrdfAsset>,
        Arc<Mutex<UrdfLoadRequestParams>>,
    )>,
);

#[derive(Message, Debug)]
pub struct UrdfAssetLoadedMessage(
    pub(crate)  (
        Handle<assets_loader::urdf::UrdfAsset>,
        Arc<Mutex<UrdfLoadRequestParams>>,
    ),
);

pub fn plugin(app: &mut App) {
    app
        // .init_state::<UrdfLoadState>()
        .add_message::<UrdfLoadRequest>()
        .add_message::<UrdfAssetLoadedMessage>()
        .init_resource::<PendingUrdfAsset>()
        .add_plugins(assets_loader::urdf::plugin)
        // handle incoming request to load urdf
        .add_systems(
            Update,
            load_urdf_request_handler.run_if(on_message::<UrdfLoadRequest>),
        )
        // check the loading state
        .add_systems(
            Update,
            track_urdf_loading_state
                // .pipe(error_to_toast)
                .run_if(|pending_urdf_asset: Res<PendingUrdfAsset>| {
                    !pending_urdf_asset.0.is_empty()
                }),
        )
        // process the loaded asset
        .add_systems(
            Update,
            load_urdf_meshes
                // .pipe(error_to_toast)
                .run_if(on_message::<UrdfAssetLoadedMessage>),
        );
}

/// request asset server to begin the load
fn load_urdf_request_handler(
    mut reader: MessageReader<UrdfLoadRequest>,
    asset_server: Res<AssetServer>,
    mut pending_urdf_asset: ResMut<PendingUrdfAsset>,
) {
    for event in reader.read() {
        pending_urdf_asset.0.push((
            asset_server.load(event.filename.clone()),
            event.params.clone(),
        ));
    }
}

/// This keep polling the asset server to check if the asset is loaded.
/// If it is loaded, then it will trigger the next event.
fn track_urdf_loading_state(
    server: Res<AssetServer>,
    mut pending_urdf_asset: ResMut<PendingUrdfAsset>,
    mut writer: MessageWriter<UrdfAssetLoadedMessage>,
) -> Result<()> {
    let original_length = pending_urdf_asset.0.len();
    {
        let pending_urdf_asset = pending_urdf_asset.bypass_change_detection();

        let mut tmp_vec = std::mem::take(&mut pending_urdf_asset.0);

        for val in &mut tmp_vec.drain(..) {
            match server.get_load_states(val.0.id()) {
                Some((_, _, bevy::asset::RecursiveDependencyLoadState::Loaded)) => {
                    writer.write(UrdfAssetLoadedMessage(val));
                }
                Some((_, _, bevy::asset::RecursiveDependencyLoadState::Failed(err))) => {
                    return Err(UrdfAssetLoadingError::FailedToLoadUrdfAsset(err).into());
                }
                _ => pending_urdf_asset.0.push(val),
            };
        }
    }
    if original_length != pending_urdf_asset.0.len() {
        // now triggers the changes
        pending_urdf_asset.set_changed();
    }
    Ok(())
}

#[derive(Component, Debug, Default)]
pub struct RobotLinkPartContainer;

/// This is a marker to denote that this is a link part
#[derive(Component, Debug, Default)]
pub struct RobotLinkPart;

/// This component should store the strong handle for each of these materials,
/// so that we can swap them
#[derive(Component, Debug, Default)]
pub struct UrdfLinkMaterial {
    /// a robot link can have an optional material tag
    pub from_inline_tag: Option<Handle<StandardMaterial>>,
    /// each link can have nested elements, which can have their own material
    pub from_mesh_component: Option<Handle<StandardMaterial>>,
}

/// A marker component to indicate that this entity is collidable
#[derive(Component, Debug)]
pub struct Collidable;

struct VisualOrCollisionContainer<'a> {
    pub name: &'a Option<String>,
    pub origin: &'a Pose,
    pub geometry: &'a Geometry,
    // pub material: Option<&'a urdf_rs::Material>,
}

/// This is a helper function to spawn a link component.
#[inline]
fn spawn_link_component_inner(
    mut entity: EntityCommands<'_>,
    mesh_handle: Handle<Mesh>,
    material_handle: Handle<StandardMaterial>,
) -> EntityCommands<'_> {
    entity.insert((Mesh3d(mesh_handle), MeshMaterial3d(material_handle)));
    entity
}

/// one robot link can have multiple visual or collision elements. This spawns
/// a unit of element
#[allow(clippy::too_many_arguments)]
fn spawn_link_component(
    link_entity: &mut EntityCommands,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    robot_materials_registry: &mut HashMap<String, Handle<StandardMaterial>>,
    prefab_assets: &Res<PrefabAssets>,
    link_components: UrdfLinkComponents,
    element_container: VisualOrCollisionContainer,
) -> Entity {
    link_entity.insert(RobotLinkPartContainer);
    let origin_element = element_container.origin;

    let mut entity_transform = Transform {
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
    };
    if let Some(name) = element_container.name {
        link_entity.insert(Name::new(name.clone()));
    }

    let link_material = link_components.link_material.map(|m| {
        match m.material {
            Some(material) => {
                let handle = materials.add(material);
                // store it in the registry (can be used by other elements in subsequent components)
                robot_materials_registry.insert(m.name.clone(), handle.clone());
                handle
            }
            None => {
                // try to retrieve from shared registry (i.e. defined earlier in the urdf)
                robot_materials_registry
                    .get(&m.name)
                    .expect("material not found in robot's materials registry")
                    .clone()
            }
        }
    });

    link_entity.with_children(|child_builder| {
        match element_container.geometry {
            // if it is a mesh, they should have been pre-loaded.
            Geometry::Mesh { filename: _, scale } => {
                if let Some(val) = scale {
                    entity_transform.scale = Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32);
                }

                let mut meshes_and_materials = link_components
                    .individual_meshes
                    .expect("if this is a mesh, it should have been pre-loaded");

                meshes_and_materials.drain(..).for_each(|(mesh, material)| {
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
                        (_, Some(material)) => material.clone(), // prortise material from mesh component
                        (Some(material), None) => material.clone(),
                        // instead of using prefab default material, we will use separate material instead,
                        // so that we can change the color of the link individually
                        (None, None) => materials.add(StandardMaterial::default()),
                        // (None, None) => prefab_assets.default_material.clone_weak(),
                    };

                    let child = child_builder.spawn_empty();

                    let mut child = spawn_link_component_inner(child, meshes.add(mesh), m_handle);
                    child.insert(material_component).insert(RobotLinkPart);
                });
            }

            primitive_geometry => {
                let handle = match prefab_assets
                    .get_prefab_mesh_handle_and_scale_from_urdf_geom(primitive_geometry)
                {
                    Some((scale, prefab_handle)) => {
                        entity_transform.scale = scale;
                        prefab_handle.clone()
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
                entity_transform = entity_transform.swap_yz_axis_and_flip_hand();

                let mut child = spawn_link_component_inner(
                    child_builder.spawn_empty(),
                    handle,
                    link_material.unwrap_or_else(|| prefab_assets.default_material.clone()),
                );
                child.insert(RobotLinkPart);
            }
        }
    });
    link_entity.insert((entity_transform, Visibility::default()));

    link_entity.id()
}

/// This is a helper function to get entity by name
fn get_entity_by_name(entities: &Query<(Entity, &Name)>, name: &str) -> Option<Entity> {
    entities
        .iter()
        .filter_map(|(e, n)| if n.as_str() == name { Some(e) } else { None })
        .next()
}

/// this gets triggers on event UrdfAssetLoadedMessage (which checks that handles are loaded)
fn load_urdf_meshes(
    mut commands: Commands,
    // mut toasts: ResMut<EguiToasts>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    prefab_assets: Res<PrefabAssets>,
    mut urdf_assets: ResMut<Assets<UrdfAsset>>,
    mut reader: MessageMutator<UrdfAssetLoadedMessage>,
) -> Result<()> {
    for event in reader.read() {
        let (handle, params) = &mut event.0;
        if let Some(UrdfAsset {
            robot: urdf_robot,
            link_meshes_materials: mut meshes_and_materials,
            mut root_materials,
        }) = urdf_assets.remove(handle)
        {
            let mut params = params.lock().unwrap();
            let mut robot_state = RobotState::new(urdf_robot.clone(), [].into());

            // apply any user-provided configuration
            let kinematic = &mut robot_state.robot_chain;
            for node in kinematic.iter() {
                let user_sepicified_joint = {
                    let joint = node.joint();
                    let name = &joint.name;
                    params.initial_joint_values.remove(name)
                };
                if let Some(joint_value) = user_sepicified_joint {
                    node.set_joint_position(joint_value).unwrap_or_else(|_| {
                        panic!("failed to set joint value for {}", &node.joint().name)
                    })
                }
            }

            //////////////////////////////////////////
            // collect info
            let child_to_parent = &mut robot_state.child_to_parent;

            for joint in urdf_robot.joints.iter() {
                child_to_parent.insert(joint.child.link.clone(), joint.parent.link.clone());
            }
            // root link is one without parent
            let root_link_name = urdf_robot
                .links
                .iter()
                .filter_map(|l| {
                    let name = &l.name;
                    match child_to_parent.get(name.as_str()) {
                        None => Some(name),
                        _ => None,
                    }
                })
                .next()
                .expect("cannot find root link");
            //////////////////////////////////////////

            ////////////////////////////////////////////////////////////////
            // apply a rotation to the urdf robot
            // first node must be root
            let origin = robot_state.robot_chain.origin();
            robot_state
                .robot_chain
                .set_origin(origin.swap_yz_axis_and_flip_hand());
            // for the workaround later, to un-apply the transformation
            let origin_anti_transform = Transform::default().swap_yz_axis_and_flip_hand_inverse();

            ////////////////////////////////////////////////////////////////
            let mut link_names_to_node = robot_state
                .robot_chain
                .iter()
                .filter_map(|n| n.link().as_ref().map(|link| (link.name.clone(), n.clone())))
                .collect::<HashMap<_, _>>();
            ////////////////////////////////////////////////////////////////

            // we will treat the root materials as a registry of materials
            // we are adding all materials to our assets here.
            let mut robot_materials_registry = root_materials
                .drain()
                .map(|(name, material)| (name, materials.add(material)))
                .collect::<HashMap<_, _>>();

            let mut robot_root = commands.spawn(RobotRoot);
            robot_root
                .insert(Name::new(urdf_robot.name))
                .insert((
                    // params.clone(),
                    params.transform,
                ))
                .with_children(|child_builder: &mut RelatedSpawnerCommands<_>| {
                    for (i, link) in urdf_robot.links.iter().enumerate() {
                        // the following is to workaround an issue (potentially a bug in urdf-rs crate??)
                        // where, after setting the origin of the root link, the origin of the root node (node without parent)
                        // seems to be not obeying the newly set origin. The rest (child nodes) are all looking fine tho.
                        // Most robots has a empty root node (node without mesh, e.g., world link) where this is a non-issue.
                        // but for robot that defines itself with a non-empty root link, the visual seems to be off.
                        // Therefore, here, we manually apply the transformation to the first node to be an anti-version of what we did
                        // previously to the origin. Note: why anti?? Idk..
                        let node_transform = if link.name.as_str() == root_link_name {
                            origin_anti_transform
                        } else {
                            Transform::default()
                        };

                        let node = link_names_to_node.remove(link.name.as_str());
                        let joint_name = node.as_ref().map(|n| n.joint().name.clone());

                        let mut robot_link_entity = child_builder.spawn(RobotLink::new(node));

                        robot_state
                            .link_names_to_entity
                            .insert(link.name.clone(), robot_link_entity.id());

                        robot_link_entity
                            .insert((node_transform, Visibility::default()))
                            .with_children(|child_builder| {
                                child_builder
                                    .spawn(RobotLinkMeshesType::Visual)
                                    .insert((Name::new(format!("{}_visual", link.name)),))
                                    .with_children(|child_builder| {
                                        for (j, visual) in link.visual.iter().enumerate() {
                                            let mesh_material_key =
                                                &(RobotLinkMeshesType::Visual, i, j);

                                            let link_components = meshes_and_materials
                                                .remove(mesh_material_key)
                                                .expect("should have been pre-loaded");

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
                                                },
                                            );
                                        }
                                    });

                                child_builder
                                    .spawn((RobotLinkMeshesType::Collision, Visibility::Hidden))
                                    .insert(Name::new(format!("{}_collision", link.name)))
                                    .with_children(|child_builder| {
                                        for (j, collision) in link.collision.iter().enumerate() {
                                            let mesh_material_key =
                                                &(RobotLinkMeshesType::Collision, i, j);
                                            let link_components = meshes_and_materials
                                                .remove(mesh_material_key)
                                                .expect("should have been pre-loaded");

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
                                                },
                                            );
                                        }
                                    });
                            })
                            .insert(Name::new(link.name.clone()));

                        if let Some(joint_name) = joint_name {
                            if let Some(init_option) =
                                params.joint_init_options.remove(joint_name.as_str())
                            {
                                robot_link_entity.insert(init_option);
                            }
                        }
                    }
                });
            robot_root.insert(robot_state);

            // // check if there are any unused params. If there are, then we will show a warning
            // if !params.initial_joint_values.is_empty() {
            //     toasts.0.warning(format!(
            //         "Some initial joint values are not used: {:?}",
            //         params.initial_joint_values.keys()
            //     ));
            // }

            // if !params.joint_init_options.is_empty() {
            //     toasts.0.warning(format!(
            //         "Some joint init options are not used: {:?}",
            //         params.joint_init_options.keys()
            //     ));
            // }
        } else {
            Err(UrdfAssetLoadingError::MissingUrdfAsset)?;
        };
    }
    Ok(())
}
