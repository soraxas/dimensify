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
pub enum GeometryType {
    Visual,
    Collision,
}

/// Represents material within a urdf format.
/// This can contains just a name (within a link element),
/// which is supposed to refers to a material defined in the
/// root of the urdf file.
#[derive(Debug, Clone)]
pub struct UrdfMaterial {
    pub name: String,
    pub material: Option<StandardMaterial>,
}

#[derive(Debug)]
pub struct UrdfLinkComponents {
    pub individual_meshes: Option<Vec<(Mesh, Option<StandardMaterial>)>>,
    pub link_material: Option<UrdfMaterial>,
}

pub type MeshMaterialMappingKey = (GeometryType, usize, usize);

pub type MeshMaterialMapping = HashMap<MeshMaterialMappingKey, UrdfLinkComponents>;

#[derive(Asset, TypePath, Debug)]
pub(crate) struct UrdfAsset {
    pub robot: Robot,
    pub link_meshes_materials: MeshMaterialMapping,
    // represents the materials listed at URDF root (can be referred by links)
    pub root_materials: HashMap<String, StandardMaterial>,
}

/// Possible errors that can be produced by [`UrdfAssetLoader`]
#[non_exhaustive]
#[derive(Debug, Error)]
enum UrdfAssetLoaderError {
    /// An [IO](std::io) Error
    #[error("Could not load asset: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse mesh asset")]
    ParsingError,
    #[error("Failed to parse bytes: {0}")]
    BevyError(#[from] bevy::asset::ReadAssetBytesError),
}

#[derive(Default)]
struct UrdfAssetLoader;

/// material_element: refers to URDF format with named materials registered in the file root,
/// which can then be used later in the link to refer to the registered material
fn load_meshes(
    scene: mesh_loader::Scene,
    // asset_server: &Res<AssetServer>,
    load_context: &mut LoadContext,
    // label: &str,
) -> Vec<(Mesh, Option<StandardMaterial>)> {
    let mut meshes = Vec::new();

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

        meshes.push((mesh_builder, material));
    }

    meshes
}

// only actually create the material if at least one of the fields is present
fn extract_urdf_material(
    material_element: &urdf_rs::Material,
    load_context: &mut LoadContext<'_>,
) -> Option<StandardMaterial> {
    if material_element.texture.is_none() && material_element.color.is_none() {
        None
    } else {
        let mut material = StandardMaterial {
            base_color_texture: material_element
                .texture
                .as_ref()
                .map(|texture| load_context.load(&texture.filename)),
            ..Default::default()
        };

        if let Some(color) = &material_element.color {
            material.base_color = Color::srgba(
                color.rgba[0] as f32,
                color.rgba[1] as f32,
                color.rgba[2] as f32,
                color.rgba[3] as f32,
            );
        }
        Some(material)
    }
}

async fn process_meshes<'a, GeomIterator, P>(
    iterator: GeomIterator,
    load_context: &mut LoadContext<'_>,
    meshes_and_materials: &mut MeshMaterialMapping,
    base_dir: &Option<P>,
    geom_type: GeometryType,
    link_idx: usize,
) -> Result<(), UrdfAssetLoaderError>
where
    GeomIterator: Iterator<Item = (&'a urdf_rs::Geometry, Option<&'a urdf_rs::Material>)>,
    P: std::fmt::Display,
{
    for (j, (geom_element, material)) in iterator.enumerate() {
        let link_material = if let Some(material_element) = material {
            Some(UrdfMaterial {
                name: material_element.name.clone(),
                material: extract_urdf_material(material_element, load_context),
            })
        } else {
            None
        };

        // try to load any mesh
        let meshes = match geom_element {
            urdf_rs::Geometry::Mesh {
                ref filename,
                scale: _,
            } => {
                // try to replace any filename with prefix, and correctly handle relative paths
                let filename = replace_package_with_base_dir(filename, base_dir);

                let bytes = load_context.read_asset_bytes(&filename).await?;
                let loader = mesh_loader::Loader::default();
                let scene = loader.load_from_slice(&bytes, &filename)?;

                Some(load_meshes(scene, load_context))
            }
            _ => None,
        };

        meshes_and_materials.insert(
            (geom_type, link_idx, j),
            UrdfLinkComponents {
                individual_meshes: meshes,
                link_material,
            },
        );
    }
    Ok(())
}

impl AssetLoader for UrdfAssetLoader {
    type Asset = UrdfAsset;
    type Settings = ();
    type Error = UrdfAssetLoaderError;

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
                    GeometryType::Collision,
                    link_idx,
                )
                .await?;

                process_meshes(
                    l.visual
                        .iter()
                        .map(|item| (&item.geometry, item.material.as_ref())),
                    load_context,
                    &mut meshes_and_materials,
                    &base_dir,
                    GeometryType::Visual,
                    link_idx,
                )
                .await?;
            }

            Ok(UrdfAsset {
                link_meshes_materials: meshes_and_materials,
                root_materials: urdf_robot
                    .materials
                    .iter()
                    .map(|m| {
                        (
                            m.name.clone(),
                            extract_urdf_material(m, load_context)
                                .expect("root material are not supposed to be empty?"),
                        )
                    })
                    .collect(),
                robot: urdf_robot,
            })
        } else {
            Err(UrdfAssetLoaderError::ParsingError)
        }
        // let custom_asset = ron::de::from_bytes::<UrdfAsset>(&bytes)?;
        // Ok(custom_asset)
    }

    fn extensions(&self) -> &[&str] {
        &["urdf", "URDF"]
    }
}
