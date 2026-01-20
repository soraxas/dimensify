use std::collections::HashMap;

use bevy::{
    asset::{AssetLoader, LoadContext, RenderAssetUsages, io::Reader},
    mesh::Indices,
    prelude::*,
    reflect::TypePath,
    render::render_resource::PrimitiveTopology,
};

use thiserror::Error;

use crate::{
    robot::RobotLinkMeshesType,
    util::{UrlParentDirGenerator, replace_package_with_base_dir},
};

use urdf_rs::Robot;

pub(crate) fn plugin(app: &mut App) {
    app.init_asset::<UrdfAsset>()
        .init_asset_loader::<UrdfAssetLoader>();
}

/// Represents material within a urdf format.
/// This can contain just a name (within a link element),
/// which is supposed to refers to a material defined in the
/// root of the urdf file.
#[derive(Debug, Clone)]
pub struct UrdfMaterial {
    pub name: String,
    pub material: Option<StandardMaterial>,
}

/// A urdf link can contain one material,
/// and multiple inner meshes (each with its own optional material).
#[derive(Debug)]
pub struct UrdfLinkComponents {
    pub individual_meshes: Option<Vec<(Mesh, Option<StandardMaterial>)>>,
    pub link_material: Option<UrdfMaterial>,
}

/// container for all the meshes and materials that are part of a urdf file.
/// we preloaded all the meshes and materials, so that we can easily
/// access them when we need to create the entities. (in an async context)
pub type MeshMaterialMappingKey = (RobotLinkMeshesType, usize, usize);
pub type MeshMaterialMapping = HashMap<MeshMaterialMappingKey, UrdfLinkComponents>;

#[derive(Asset, TypePath, Debug)]
pub(crate) struct UrdfAsset {
    pub robot: Robot,
    pub link_meshes_materials: MeshMaterialMapping,
    /// represents the materials listed at URDF root (can be referred by links)
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

/// Load the meshes from the scene, and return a list of meshes and materials.
fn load_meshes(
    scene: mesh_loader::Scene,
    load_context: &mut LoadContext,
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

/// only actually create the material if at least one of the fields is present
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

/// A helper function to try to load a mesh from different potential URLs, and
/// return the meshes and materials if successful, as well as the base_url that works.
async fn load_from_potential_parent_url(
    base_dir: &str,
    filename: &str,
    load_context: &mut LoadContext<'_>,
) -> Option<(Vec<(Mesh, Option<StandardMaterial>)>, String)> {
    // try to replace any filename with prefix
    // e.g., if base_url is https://example.com/a/b/c, and filename is d/e/f.stl, we will try to load
    // the file from https://example.com/a/b/c/d/e/f.stl, https://example.com/a/b/d/e/f.stl,
    // https://example.com/a/d/e/f.stl, https://example.com/d/e/f.stl, and https://example.com/d/e/f.stl.
    for potential_url_parent in UrlParentDirGenerator::new(base_dir) {
        let path = format!("{}/{}", potential_url_parent, filename);

        debug!("trying to load from potential url: {}", &path);
        if let Ok(bytes) = load_context.read_asset_bytes(&path).await {
            info!("loaded mesh from potential url: {}", &path);
            let loader = mesh_loader::Loader::default();
            if let Ok(scene) = loader.load_from_slice(&bytes, &path) {
                return Some((load_meshes(scene, load_context), potential_url_parent));
            }
        }
    }
    None
}

/// Process the meshes of a link, and store them in the `meshes_and_materials` hashmap.
/// `base_dir` is is mutable, as it used to resolve the path of the mesh files, and
/// this function can discover the correct base_dir by trying to load the mesh iteratively
/// by walking up the path. (only if the mesh filename is prefixed with `package://`)
async fn process_meshes<'a, GeomIterator>(
    iterator: GeomIterator,
    load_context: &mut LoadContext<'_>,
    meshes_and_materials: &mut MeshMaterialMapping,
    base_dir: &mut Option<String>,
    geom_type: RobotLinkMeshesType,
    link_idx: usize,
) -> Result<(), UrdfAssetLoaderError>
where
    GeomIterator: Iterator<Item = (&'a urdf_rs::Geometry, Option<&'a urdf_rs::Material>)>,
{
    for (j, (geom_element, material)) in iterator.enumerate() {
        let link_material = material.map(|material_element| UrdfMaterial {
            name: material_element.name.clone(),
            material: extract_urdf_material(material_element, load_context),
        });

        // try to load any mesh
        let meshes = match geom_element {
            urdf_rs::Geometry::Mesh { filename, scale: _ } => {
                // try to replace any filename with prefix, and correctly handle relative paths
                match (filename.strip_prefix("package://"), &base_dir) {
                    // if there are base_dir but the mesh filename is prefixed with `package://`,
                    // we have no way to know the actual root-path of the file, so we need to try to walk back
                    // to the base_dir step-by-step and try to load the file from there.
                    (Some(stripped_filename), Some(potential_base_dir)) => {
                        if let Some((meshes, working_parent_url)) = load_from_potential_parent_url(
                            potential_base_dir.as_str(),
                            stripped_filename,
                            load_context,
                        )
                        .await
                        {
                            // we found the a correct base_dir (note: if there are duplicated filename, this
                            // can potentially be incorrect, and leads to an incorrect greedy approach)
                            info!("updating base_dir: {}", &working_parent_url);
                            // the updated base_dir will be used for the next mesh loading
                            // if the assets is hosted online, this avoids too many request to remote server,
                            // and avoid bot-like behavior.
                            *base_dir = Some(working_parent_url);
                            Some(meshes)
                        } else {
                            return Err(UrdfAssetLoaderError::Io(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!(
                                    "Could not load the given file when processing {}. The file has `package://` prefix, and we've tried different parent subdir.",
                                    filename
                                ),
                            )));
                        }
                    }
                    // base case of direct loading
                    _ => {
                        let filename = replace_package_with_base_dir(filename, base_dir);
                        info!("Loading mesh: {}", &filename);

                        let bytes = load_context.read_asset_bytes(&filename).await?;
                        let loader = mesh_loader::Loader::default();
                        let scene = loader.load_from_slice(&bytes, &filename)?;

                        Some(load_meshes(scene, load_context))
                    }
                }
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

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        if let Some(urdf_robot) = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|utf| urdf_rs::read_from_string(utf).ok())
        {
            let mut base_dir = load_context.asset_path().parent().map(|p| p.to_string());

            let mut meshes_and_materials = MeshMaterialMapping::new();

            // let mut vector =  Vec::new();
            for (link_idx, l) in urdf_robot.links.iter().enumerate() {
                process_meshes(
                    l.collision.iter().map(|item| (&item.geometry, None)),
                    load_context,
                    &mut meshes_and_materials,
                    &mut base_dir,
                    RobotLinkMeshesType::Collision,
                    link_idx,
                )
                .await?;

                process_meshes(
                    l.visual
                        .iter()
                        .map(|item| (&item.geometry, item.material.as_ref())),
                    load_context,
                    &mut meshes_and_materials,
                    &mut base_dir,
                    RobotLinkMeshesType::Visual,
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
