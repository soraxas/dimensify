use bevy::prelude::*;

pub mod helpers;
pub mod prefab_mesh;

pub fn plugin(app: &mut App) {
    // a.initialise_if_empty(meshes);

    app.insert_resource(prefab_mesh::PrefabMesh::default())
        .add_systems(Startup, initialise_prefab_meshes);
}

fn initialise_prefab_meshes(
    mut meshes: ResMut<prefab_mesh::PrefabMesh>,
    mut meshes_res: ResMut<Assets<Mesh>>,
) {
    meshes.initialise_if_empty(&mut meshes_res);
}
