use bevy::{prelude::*, transform::commands};

pub mod helpers;
pub mod prefab_assets;

pub fn plugin(app: &mut App) {
    // a.initialise_if_empty(meshes);

    app.add_systems(Startup, initialise_prefab_assets);
}

fn initialise_prefab_assets(
    mut commands: Commands,
    mut meshes_res: ResMut<Assets<Mesh>>,
    mut materials_res: ResMut<Assets<StandardMaterial>>,
) {
    // meshes.initialise_if_empty(&mut meshes_res, &mut materials_res);

    commands.insert_resource(prefab_assets::PrefabAssets::new(
        &mut meshes_res,
        &mut materials_res,
    ));
}
