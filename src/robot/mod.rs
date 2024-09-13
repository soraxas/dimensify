use bevy::{
    app::{App, Startup},
    math::sampling::standard,
};

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

use super::assets_loader::{self, rgba_from_visual};

// use super::assets_loader::{self, rgba_from_visual};
use bevy_asset_loader::prelude::*;

use k::{self, urdf};

#[derive(Resource)]
struct RobotHandle(HashMap<String, Handle<assets_loader::mesh::MeshAsset>>);

#[derive(Resource, Debug)]
pub struct RobotState {
    pub urdf_robot: Robot,
    pub end_link_names: Vec<String>,
    pub is_collision: bool,
    pub disable_texture: bool,
    pub robot_chain: k::Chain<f32>,
    pub link_names_to_entity: HashMap<String, Entity>,
    pub joint_link_map: HashMap<String, String>,
}

impl RobotState {
    pub fn new(
        urdf_robot: Robot,
        end_link_names: Vec<String>,
        //
    ) -> Self {
        // let joint_link_map = k::urdf::joint_to_link_map(&urdf_robot);

        Self {
            joint_link_map: k::urdf::joint_to_link_map(&urdf_robot),
            robot_chain: urdf_robot.clone().into(),
            urdf_robot,
            end_link_names,
            is_collision: false,
            disable_texture: false,
            // link_joint_map: k::urdf::link_to_joint_map(&urdf_robot),
            link_names_to_entity: Default::default(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Hash, Default, States)]
enum UrdfLoadState {
    #[default]
    UrdfSetup,
    UrdfLoading,
    MeshSetup,
    MeshLoading,
    Next,
}

use bevy_asset_loader::prelude::*;

use bevy_asset_loader::dynamic_asset::DynamicAsset;
use bevy_asset_loader::standard_dynamic_asset::StandardDynamicAsset;

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
    dbg!("heyyyy");
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

fn spawn_link(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    meshes: &mut ResMut<Assets<Mesh>>,
    link: &urdf_rs::Link,
    mesh_material_key: &assets_loader::urdf::MeshMaterialMappingKey,
    robot_state: &mut RobotState,
    standard_default_material: &mut Option<Handle<StandardMaterial>>,
    meshes_and_materials: &mut assets_loader::urdf::MeshMaterialMapping,
    geom_element: &Geometry,
    origin_element: &Pose,
) {
    match *geom_element {
        urdf_rs::Geometry::Mesh { filename: _, scale } => {
            let scale = scale.map_or_else(
                || Vec3::ONE,
                |val| Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32),
            );

            // dbg!(origin_element);
            // dbg!(&urdf_asset.meshes_and_materials);

            let mut entity = commands.spawn_empty();

            robot_state
                .link_names_to_entity
                .insert(link.name.clone(), entity.id());

            entity
                .insert(RobotBundle {
                    spatial: SpatialBundle {
                        transform: Transform {
                            translation: Vec3::new(
                                origin_element.xyz[0] as f32,
                                origin_element.xyz[2] as f32,
                                origin_element.xyz[1] as f32,
                            ),
                            rotation: Quat::from_euler(
                                EulerRot::XYZ,
                                origin_element.rpy[0] as f32,
                                origin_element.rpy[2] as f32,
                                origin_element.rpy[1] as f32,
                            ),
                            scale: scale,
                        },
                        ..default()
                    },
                    ..default()
                })
                .with_children(|builder| {
                    if let Some(mut meshes_and_materials) =
                        meshes_and_materials.remove(mesh_material_key)
                    {
                        // dbg!("heyy");
                        // dbg!("heyy", i, j);
                        // dbg!("heyy", i, j, &meshes_and_materials);
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
                                    standard_default_material.as_ref().unwrap().clone()
                                }
                            };

                            // if let Some(material) = material {
                            //     bundle.material = materials.add(material);
                            // }
                            builder.spawn(bundle);
                        });
                    }
                });
        }
        _ => {
            todo!();
        }
    }
}

fn load_urdf_meshes(
    mut commands: Commands,
    mut state: ResMut<NextState<UrdfLoadState>>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    urdf_asset_loader: Res<UrdfAssetCollection>,
    mut urdf_assets: ResMut<Assets<UrdfAsset>>,
) {
    let is_collision = false;
    // let is_collision = true;

    let mut urdf_asset = urdf_assets.remove(&urdf_asset_loader.urdf).unwrap();

    let urdf_robot = urdf_asset.robot;
    let mut meshes_and_materials = urdf_asset.meshes_and_materials;

    let mut robot_state = RobotState::new(urdf_robot.clone(), [].into());

    let mut standard_default_material = None;

    for (i, l) in urdf_robot.links.iter().enumerate() {
        let use_mesh_type = assets_loader::urdf::MeshType::Visual;

        let element_types = l.visual.iter();
        let mesh_type = assets_loader::urdf::MeshType::Visual;

        for (j, visual) in l.visual.iter().enumerate() {
            let mesh_material_key = &(mesh_type, i, j);
            spawn_link(
                &mut commands,
                &mut materials,
                &mut meshes,
                l,
                mesh_material_key,
                &mut robot_state,
                &mut standard_default_material,
                &mut meshes_and_materials,
                &visual.geometry,
                &visual.origin,
            );
        }

        for (j, (geom_element, origin_element)) in element_types
            .map(|item| (&item.geometry, &item.origin))
            .enumerate()
        {
            let mesh_material_key = &(mesh_type, i, j);
            spawn_link(
                &mut commands,
                &mut materials,
                &mut meshes,
                l,
                mesh_material_key,
                &mut robot_state,
                &mut standard_default_material,
                &mut meshes_and_materials,
                geom_element,
                origin_element,
            );
        }
    }

    commands.insert_resource(robot_state);

    state.set(UrdfLoadState::Next);
}

pub fn plugin(app: &mut App) {
    debug!("Registering robot plugin");

    let path = "assets/panda/urdf/panda_relative.urdf";

    app
        //     .insert_resource(RobotState::new(
        //     robot.expect("cannot load robot"),
        //     [].into(),
        //     //
        // ))
        .init_state::<UrdfLoadState>()
        .add_plugins(assets_loader::urdf::plugin)
        .add_loading_state(
            LoadingState::new(UrdfLoadState::UrdfLoading)
                .continue_to_state(UrdfLoadState::MeshSetup)
                .load_collection::<UrdfAssetCollection>(),
        )
        .add_systems(
            Update,
            (
                load_urdf.run_if(in_state(UrdfLoadState::UrdfSetup)),
                // load_urdf_meshes.run_if(in_state(UrdfLoadState::MeshLoading)),
                // update_menu.run_if(in_state(MyStates::Menu)),
                // move_player.run_if(in_state(MyStates::Next)),
            ),
        )
        .add_systems(OnEnter(UrdfLoadState::MeshSetup), (load_urdf_meshes,))
        // .init_state::<UrdfLoadState>()
        // .add_loading_state(
        //     LoadingState::new(UrdfLoadState::AssetLoading)
        //         .continue_to_state(UrdfLoadState::Next)
        //         .load_collection::<AudioAssets>(),
        // )
        // .add_systems(OnEnter(UrdfLoadState::Next), start_background_audio)
        // .add_plugins(assets_loader::urdf::plugin)
        // .add_systems(Startup, setup);
        .add_systems(Startup, setup);
}

/// Marker
#[derive(Component, Default)]
struct RobotLink;

#[derive(Bundle, Default)]
pub struct RobotBundle {
    pub spatial: SpatialBundle,
    _link: RobotLink,
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    // mut robot_state: ResMut<RobotState>,
) {
    // let robot_path = "assets/panda/urdf/panda.urdf";
    // let robot_path = "assets/panda/urdf/panda.urdf";

    // // let urdf_robot = urdf_rs::utils::read_urdf_or_xacro(&robot_path).expect("robot file not found");

    // let urdf_robot: Handle<assets_loader::urdf::UrdfAsset> = asset_server.load(robot_path);

    // urdf_robot.

    return;

    // let is_collision = false;
    // // let is_collision = true;

    // let mut iii = 0;

    // for l in &urdf_robot.links {
    //     let num = if is_collision {
    //         l.collision.len()
    //     } else {
    //         l.visual.len()
    //     };
    //     if num == 0 {
    //         continue;
    //     }
    //     // let mut scene_group = window.add_group();
    //     // let mut colors = Vec::new();
    //     for i in 0..num {
    //         let (geom_element, origin_element) = if is_collision {
    //             (&l.collision[i].geometry, &l.collision[i].origin)
    //         } else {
    //             (&l.visual[i].geometry, &l.visual[i].origin)
    //         };

    //         let mut material = StandardMaterial {
    //             // base_color_texture: Some(custom_texture_handle),
    //             ..default()
    //         };

    //         // let mut opt_color = None;
    //         if l.visual.len() > i {
    //             let rgba = rgba_from_visual(&urdf_robot, &l.visual[i]);
    //             // // let color = na::Point3::new(rgba[0] as f32, rgba[1] as f32, rgba[2] as f32);
    //             if rgba[0] > 0.001 || rgba[1] > 0.001 || rgba[2] > 0.001 {
    //                 // opt_color = Some(color);
    //                 material.base_color = Color::srgba(
    //                     rgba[0] as f32,
    //                     rgba[1] as f32,
    //                     rgba[2] as f32,
    //                     rgba[3] as f32,
    //                 );
    //             }
    //             // colors.push(color);
    //         }

    //         match *geom_element {
    //             urdf_rs::Geometry::Mesh {
    //                 ref filename,
    //                 scale,
    //             } => {
    //                 dbg!(filename);

    //                 let filename = filename
    //                     .strip_prefix("package://robot_resources")
    //                     .unwrap_or(filename);

    //                 let filename = format!("assets/{}", filename);

    //                 let mut __meshes = load_meshes(filename.as_str(), &asset_server);

    //                 let scale = scale.map_or_else(
    //                     || Vec3::ONE,
    //                     |val| Vec3::new(val[0] as f32, val[1] as f32, val[2] as f32),
    //                 );

    //                 dbg!(origin_element);

    //                 let mut entity = commands.spawn_empty();

    //                 robot_state
    //                     .link_names_to_entity
    //                     .insert(l.name.clone(), entity.id());

    //                 iii += 1;
    //                 entity
    //                     .insert(RobotBundle {
    //                         spatial: SpatialBundle {
    //                             transform: Transform {
    //                                 translation: Vec3::new(
    //                                     origin_element.xyz[0] as f32,
    //                                     origin_element.xyz[2] as f32,
    //                                     origin_element.xyz[1] as f32,
    //                                 ),
    //                                 rotation: Quat::from_euler(
    //                                     EulerRot::XYZ,
    //                                     origin_element.rpy[0] as f32,
    //                                     origin_element.rpy[2] as f32,
    //                                     origin_element.rpy[1] as f32,
    //                                 ),
    //                                 scale: scale,
    //                             },
    //                             ..default()
    //                         },
    //                         ..default()
    //                     })
    //                     .with_children(|builder| {
    //                         __meshes.drain(..).for_each(|(m, material)| {
    //                             let mut bundle = PbrBundle {
    //                                 mesh: meshes.add(m),
    //                                 ..default()
    //                             };
    //                             if let Some(material) = material {
    //                                 bundle.material = materials.add(material);
    //                             }
    //                             builder.spawn(bundle);
    //                         });
    //                     });
    //             }
    //             _ => {
    //                 todo!();
    //             }
    //         }

    //         // match add_geometry(
    //         //     geom_element,
    //         //     &opt_color,
    //         //     base_dir,
    //         //     &mut scene_group,
    //         //     self.is_texture_enabled,
    //         //     self.is_assimp_enabled,
    //         //     package_path,
    //         // ) {
    //         //     Ok(mut base_group) => {
    //         //         // set initial origin offset
    //         //         base_group.set_local_transformation(k::urdf::isometry_from(origin_element));
    //         //     }
    //         //     Err(e) => {
    //         //         error!("failed to create for link '{}': {e}", l.name);
    //         //     }
    //         // }
    //     }
    //     // let joint_name = self
    //     //     .link_joint_map
    //     //     .get(&l.name)
    //     //     .unwrap_or_else(|| panic!("joint for link '{}' not found", l.name));
    //     // self.scenes.insert(joint_name.to_owned(), scene_group);
    //     // self.original_colors.insert(joint_name.to_owned(), colors);
    // }
}

fn load_meshes(
    path: &str,
    asset_server: &Res<AssetServer>,
) -> Vec<(Mesh, Option<StandardMaterial>)> {
    let mut __meshes = Vec::new();

    let mut loader = mesh_loader::Loader::default();
    let scene = loader.load(path).unwrap();

    for (i, (mesh, material)) in scene.meshes.into_iter().zip(scene.materials).enumerate() {
        let mut mesh_builder = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );

        mesh_builder =
            mesh_builder.with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mesh.vertices);

        if !mesh.normals.is_empty() {
            mesh_builder =
                mesh_builder.with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, mesh.normals);
        };

        // let a = mesh.texcoords[0].iter().copied();
        // if !mesh.texcoords[0].is_empty() {
        //     mesh_builder = mesh_builder.with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, a);
        // };

        let material = match (
            &material.color.diffuse,
            &material.texture.diffuse,
            &material.texture.ambient,
        ) {
            (None, None, None) => None, // no need to build it
            (color, path_diffuse, path_ambient) => {
                let mut m = StandardMaterial::default();

                if let Some(color) = color {
                    m.base_color = Color::srgb(color[0], color[1], color[2]);
                }
                if let Some(path_diffuse) = path_diffuse {
                    m.base_color_texture = Some(asset_server.load(path_diffuse.clone()));
                }
                if let Some(path_ambient) = path_ambient {
                    m.occlusion_texture = Some(asset_server.load(path_ambient.clone()));
                }

                Some(m)
            }
        };

        mesh_builder = mesh_builder
            .with_inserted_indices(Indices::U32(mesh.faces.into_iter().flatten().collect()));

        __meshes.push((mesh_builder, material));

        // result_meshes.push(mesh_builder);

        // let labeled = load_context.labeled_asset_scope(format!("{}", i), |_| {
        //     //
        //     mesh_builder
        // });
        // dbg!(&labeled);

        // handles.push(labeled);

        // let handle = load_context.labeled_asset_scope("label".to_owned(), move |ctx| {
        //     ctx.set_default_asset(mesh_builder);
        // });
        // let handle = load_context.add_labeled_asset("label".into(), mesh_builder);

        // load_context.begin_labeled_asset()

        // load_context.set_labeled_asset("cube", LoadedAsset::new(mesh));
    }
    __meshes
}
