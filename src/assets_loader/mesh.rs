use bevy::{
    asset::{io::Reader, AssetLoader, AsyncReadExt, LoadContext},
    ecs::world,
    prelude::*,
    reflect::TypePath,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
    },
};
use serde::Deserialize;
use thiserror::Error;

use urdf_rs::Robot;

pub(crate) fn plugin(app: &mut App) {
    app.init_asset::<MeshAsset>()
        .init_asset_loader::<MeshAssetLoader>();
}

#[derive(Asset, TypePath, Debug)]
pub(crate) struct MeshAsset {
    #[allow(dead_code)]
    // robot: Robot,
    // meshes: Vec<Mesh>,
    pub meshes: Vec<Handle<Mesh>>,
}
/// Possible errors that can be produced by [`MeshAssetLoader`]
#[non_exhaustive]
#[derive(Debug, Error)]
enum MeshLoaderError {
    /// An [IO](std::io) Error
    #[error("Could not load asset: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse mesh asset")]
    ParsingError,
}

#[derive(Default)]
struct MeshAssetLoader;

impl AssetLoader for MeshAssetLoader {
    type Asset = MeshAsset;
    type Settings = ();
    type Error = MeshLoaderError;
    async fn load<'a>(
        &'a self,
        reader: &'a mut Reader<'_>,
        _settings: &'a (),
        load_context: &'a mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let use_texture = true;

        let mut loader = mesh_loader::Loader::default();
        let scene = loader.load_from_slice(&bytes, load_context.path())?;

        let mut result_meshes: Vec<Mesh> = Vec::new();

        let mut handles = Vec::new();
        // for i in 0..2 {
        //     let mut labeled = load_context.begin_labeled_asset();
        //     handles.push(std::thread::spawn(move || {
        //         (i.to_string(), labeled.finish(Image::default(), None))
        //     }));
        // }
        // for handle in handles {
        //     let (label, loaded_asset) = handle.join().unwrap();
        //     load_context.add_loaded_labeled_asset(label, loaded_asset);
        // }

        if true {
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

                if use_texture {
                    // if let Some(color) = material.color.diffuse {
                    //     kiss3d_scene.set_color(color[0], color[1], color[2]);
                    // }
                    // if let Some(path) = &material.texture.diffuse {
                    //     let path_string = path.to_str().unwrap();
                    //     // TODO: Using fetch_or_read can support remote materials, but loading becomes slow.
                    //     // let buf = fetch_or_read(path_string)?;
                    //     // kiss3d_scene.set_texture_from_memory(&buf, path_string);
                    //     kiss3d_scene.set_texture_from_file(path, path_string);
                    // }
                    // if let Some(path) = &material.texture.ambient {
                    //     let path_string = path.to_str().unwrap();
                    //     // TODO: Using fetch_or_read can support remote materials, but loading becomes slow.
                    //     // let buf = fetch_or_read(path_string)?;
                    //     // kiss3d_scene.set_texture_from_memory(&buf, path_string);
                    //     kiss3d_scene.set_texture_from_file(path, path_string);
                    // }
                }

                // mesh_builder = mesh_builder.with_inserted_indices(Indices::U32(vec![
                //     0, 3, 1, 1, 3, 2, // triangles making up the top (+y) facing side.
                //     4, 5, 7, 5, 6, 7, // bottom (-y)
                //     8, 11, 9, 9, 11, 10, // right (+x)
                //     12, 13, 15, 13, 14, 15, // left (-x)
                //     16, 19, 17, 17, 19, 18, // back (+z)
                //     20, 21, 23, 21, 22, 23, // forward (-z)
                // ]));

                mesh_builder = mesh_builder.with_inserted_indices(Indices::U32(
                    mesh.faces.into_iter().flatten().collect(),
                ));

                // result_meshes.push(mesh_builder);

                let labeled = load_context.labeled_asset_scope(format!("{}", i), |_| {
                    //
                    mesh_builder
                });
                dbg!(&labeled);

                handles.push(labeled);

                // let handle = load_context.labeled_asset_scope("label".to_owned(), move |ctx| {
                //     ctx.set_default_asset(mesh_builder);
                // });
                // let handle = load_context.add_labeled_asset("label".into(), mesh_builder);

                // load_context.begin_labeled_asset()

                // load_context.set_labeled_asset("cube", LoadedAsset::new(mesh));
            }
            // if let Some(color) = *opt_color {
            //     base.set_color(color[0], color[1], color[2]);
            // }

            let handle = MeshAsset {
                // meshes: result_meshes,
                meshes: handles,
            };

            Ok(handle)

        //     urdf_rs::read_from_string(data).ok()
        // }) {
        //     Ok(MeshAsset { robot: res })
        } else {
            Err(MeshLoaderError::ParsingError)
        }
        // let custom_asset = ron::de::from_bytes::<MeshAsset>(&bytes)?;
        // Ok(custom_asset)
    }

    fn extensions(&self) -> &[&str] {
        &["dae"]
    }
}
