use std::collections::HashMap;

use bevy::{
    asset::{io::Reader, AssetLoader, AsyncReadExt, LoadContext},
    prelude::*,
    reflect::TypePath,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
    },
};

use serde::Deserialize;
use thiserror::Error;

use crate::util::replace_package_with_base_dir;

use urdf_rs::Robot;

pub(crate) fn plugin(app: &mut App) {
    app.init_asset::<UrdfAsset>()
        .init_asset_loader::<UrdfAssetLoader>();
}

#[derive(Debug, Deserialize, PartialEq, Eq, Hash, Copy, Clone)]
pub enum MeshType {
    Visual,
    Collision,
}

// for l in &urdf_robot.links {
//     let num = if is_collision {
//         l.collision.len()
//     } else {
//         l.visual.len()
//     };
//     if num == 0 {
//         continue;
//     }

pub type MeshMaterialMappingKey = (MeshType, usize, usize);
pub type MeshMaterialMapping =
    HashMap<MeshMaterialMappingKey, Vec<(Mesh, Option<StandardMaterial>)>>;
// #[derive(Debug)]
// pub struct MeshMaterialMapping(
//     pub HashMap<(MeshType, usize, usize), Vec<(Mesh, Option<StandardMaterial>)>>,
// );

#[derive(Asset, TypePath, Debug)]
pub(crate) struct UrdfAsset {
    #[allow(dead_code)]
    pub robot: Robot,
    pub meshes_and_materials: MeshMaterialMapping,
    // pub meshes_and_materials: Vec<(
    //     urdf_rs::Geometry,
    //     Option<Vec<(Mesh, Option<StandardMaterial>)>>,
    // )>,
}
/// Possible errors that can be produced by [`UrdfAssetLoader`]
#[non_exhaustive]
#[derive(Debug, Error)]
enum CustomAssetLoaderError {
    /// An [IO](std::io) Error
    #[error("Could not load asset: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse mesh asset")]
    ParsingError,
}

#[derive(Default)]
struct UrdfAssetLoader;

fn load_meshes(
    scene: mesh_loader::Scene,
    // asset_server: &Res<AssetServer>,
    visual_material: Option<&urdf_rs::Material>,
    load_context: &mut LoadContext,
    // label: &str,
) -> Vec<(Mesh, Option<StandardMaterial>)> {
    let mut __meshes = Vec::new();

    // try to load any mesh
    if let Some(visual_material) = visual_material {
        /* <?xml version="1.0"?>
        <robot name="visual">

        <material name="blue">
            <color rgba="0 0 0.8 1"/>
          </material>
          <material name="black">
            <color rgba="0 0 0 1"/>
          </material>
          <material name="white">
            <color rgba="1 1 1 1"/>
          </material>

          <link name="base_link">
            <visual>
              <geometry>
                <cylinder length="0.6" radius="0.2"/>
              </geometry>
              <material name="blue"/>
            </visual>
          </link>

          <link name="right_leg">
            <visual>
              <geometry>
                <box size="0.6 0.1 0.2"/>
              </geometry>
              <origin rpy="0 1.57075 0" xyz="0 0 -0.3"/>
              <material name="white"/>
            </visual>
          </link> */
        todo!();
    }

    // let mut loader = mesh_loader::Loader::default();
    // let scene: mesh_loader::Scene = loader.load(path).unwrap();

    for (mesh, material) in scene.meshes.into_iter().zip(scene.materials) {
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
                    m.base_color_texture = Some(load_context.load(path_diffuse.clone()));
                }
                if let Some(path_ambient) = path_ambient {
                    m.occlusion_texture = Some(load_context.load(path_ambient.clone()));
                }

                Some(m)
            }
        };

        mesh_builder = mesh_builder
            .with_inserted_indices(Indices::U32(mesh.faces.into_iter().flatten().collect()));

        __meshes.push((mesh_builder, material));
    }
    __meshes
}

async fn process_meshes<'a, GeomIterator, P>(
    iterator: GeomIterator,
    load_context: &mut LoadContext<'_>,
    meshes_and_materials: &mut MeshMaterialMapping,
    base_dir: &Option<P>,
    mesh_type: MeshType,
    link_idx: usize,
)
// -> Result<(Mesh, Option<StandardMaterial>)>
where
    GeomIterator: Iterator<Item = (&'a urdf_rs::Geometry, Option<&'a urdf_rs::Material>)>,
    P: std::fmt::Display,
{
    // let meshes_and_materials = HashMap::new();

    for (j, (geom_element, material)) in iterator.enumerate() {
        if let urdf_rs::Geometry::Mesh {
            ref filename,
            scale: _,
        } = geom_element
        {
            // try to replace any filename with prefix, and correctly handle relative paths
            let filename = replace_package_with_base_dir(filename, base_dir);

            let meshes = match load_context.read_asset_bytes(&filename).await {
                Ok(bytes) => {
                    let loader = mesh_loader::Loader::default();
                    let scene = loader
                        .load_from_slice(&bytes, &filename)
                        .expect("failed to load mesh");
                    // scene.meshes

                    load_meshes(scene, material, load_context)
                }
                Err(e) => {
                    error!("cannot load mesh at {}: {}", &filename, e);
                    vec![]
                }
            };

            meshes_and_materials.insert((mesh_type, link_idx, j), meshes);
        };
    }
}

impl AssetLoader for UrdfAssetLoader {
    type Asset = UrdfAsset;
    type Settings = ();
    type Error = CustomAssetLoaderError;

    async fn load<'a>(
        &'a self,
        reader: &'a mut Reader<'_>,
        _settings: &'a (),
        load_context: &'a mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        if let Some(urdf_robot) = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|utf| urdf_rs::read_from_string(utf).ok())
        {
            let base_dir = load_context.asset_path().parent();

            let mut meshes_and_materials = MeshMaterialMapping::new();

            // let mut vector =  Vec::new();
            for (link_idx, l) in urdf_robot.links.iter().enumerate() {
                process_meshes(
                    l.collision.iter().map(|item| (&item.geometry, None)),
                    load_context,
                    &mut meshes_and_materials,
                    &base_dir,
                    MeshType::Collision,
                    link_idx,
                )
                .await;

                process_meshes(
                    l.visual
                        .iter()
                        .map(|item| (&item.geometry, item.material.as_ref())),
                    load_context,
                    &mut meshes_and_materials,
                    &base_dir,
                    MeshType::Visual,
                    link_idx,
                )
                .await;
            }

            Ok(UrdfAsset {
                robot: urdf_robot,
                meshes_and_materials,
            })
        } else {
            Err(CustomAssetLoaderError::ParsingError)
        }
        // let custom_asset = ron::de::from_bytes::<UrdfAsset>(&bytes)?;
        // Ok(custom_asset)
    }

    fn extensions(&self) -> &[&str] {
        &["urdf", "URDF"]
    }
}
