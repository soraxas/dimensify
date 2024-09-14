use bevy::{
    app::{App, Startup},
    math::sampling::standard,
};
use eyre::Context;

use std::{collections::HashMap, f32::consts::*, io};

use bevy::{
    core_pipeline::{
        fxaa::Fxaa,
        prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
    },
    ecs::{observer::TriggerTargets, system::SystemId},
    pbr::{
        CascadeShadowConfigBuilder, DefaultOpaqueRendererMethod, DirectionalLightShadowMap,
        NotShadowCaster, NotShadowReceiver, OpaqueRendererMethod,
    },
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
        texture::ImageLoaderSettings,
    },
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use urdf_rs::{Geometry, Pose, Robot};

use crate::assets_loader::urdf::UrdfAsset;

use super::{
    assets_loader::{self, rgba_from_visual},
    RobotLinkCollision, RobotLinkVisual, RobotRoot,
};

// use super::assets_loader::{self, rgba_from_visual};
use bevy_asset_loader::prelude::*;

use bevy_asset_loader::prelude::*;

use bevy_asset_loader::dynamic_asset::DynamicAsset;
use bevy_asset_loader::standard_dynamic_asset::StandardDynamicAsset;

use crate::robot::{RobotLink, RobotState};

#[derive(Clone, Eq, PartialEq, Debug, Hash, Default, States)]
pub(crate) enum UrdfLoadState {
    #[default]
    UrdfSetup,
    UrdfLoading,
    MeshSetup,
    Next,
}

pub fn mesh_loader_plugin(app: &mut App) {
    app.init_state::<UrdfLoadState>()
        .add_plugins(assets_loader::urdf::plugin)
        .add_loading_state(
            LoadingState::new(UrdfLoadState::UrdfLoading)
                .continue_to_state(UrdfLoadState::MeshSetup)
                .load_collection::<UrdfAssetCollection>(),
        )
        .add_systems(
            Update,
            (load_urdf.run_if(in_state(UrdfLoadState::UrdfSetup)),),
        )
        .add_systems(OnEnter(UrdfLoadState::MeshSetup), (load_urdf_meshes,));
}

#[derive(AssetCollection, Resource)]
struct UrdfAssetCollection {
    #[asset(key = "urdf")]
    urdf: Handle<assets_loader::urdf::UrdfAsset>,
}

fn load_urdf(
    mut commands: Commands,
    mut state: ResMut<NextState<UrdfLoadState>>,
    asset_server: Res<AssetServer>,
    mut dynamic_assets: ResMut<DynamicAssets>,
) {
    dynamic_assets.register_asset(
        "urdf",
        Box::new(StandardDynamicAsset::File {
            // path: "3d/T12/urdf/T12.URDF".to_owned(),
            path: "robot_resources/panda/urdf/panda.urdf".to_owned(),
            // path: "panda/urdf/panda.urdf".to_owned(),
        }),
        // "3d/T12/urdf/T12.URDF"
    );
    state.set(UrdfLoadState::UrdfLoading);
}

fn update_robot_visual(
    robot_state: Res<RobotState>,
    mut query: Query<(&RobotLink, &mut Transform)>,
) {
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
                .insert(SpatialBundle {
                        transform: Transform {
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
                        ..default()
                    })
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

fn load_urdf_meshes(
    mut commands: Commands,
    mut state: ResMut<NextState<UrdfLoadState>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    urdf_asset_loader: Res<UrdfAssetCollection>,
    mut urdf_assets: ResMut<Assets<UrdfAsset>>,
) {
    let mut urdf_asset = urdf_assets.remove(&urdf_asset_loader.urdf).unwrap(); // unwrap cannot fails as assets are always loaded when reaching here

    let urdf_robot = urdf_asset.robot;
    let mut meshes_and_materials = urdf_asset.meshes_and_materials;

    let mut robot_state = RobotState::new(urdf_robot.clone(), [].into());

    let mut standard_default_material = None;

    commands
        .spawn(RobotRoot)
        .insert(Name::new(urdf_robot.name))
        .insert(SpatialBundle::default())
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
                            .spawn(RobotLinkVisual)
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
                            .spawn(RobotLinkCollision)
                            .insert(SpatialBundle::default())
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

    commands.insert_resource(robot_state);

    state.set(UrdfLoadState::Next);
}

#[derive(Bundle, Default)]
pub struct RobotLinkBundle {
    pub spatial: SpatialBundle,
    _link: RobotLink,
}
