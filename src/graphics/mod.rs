use bevy::prelude::*;
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin, InfiniteGridSettings};

pub mod helpers;
pub mod prefab_assets;

pub fn plugin(app: &mut App) {
    // a.initialise_if_empty(meshes);

    app.add_systems(Startup, initialise_prefab_assets)
        .add_plugins(InfiniteGridPlugin)
        .add_systems(Startup, spawn_entities);
}

fn spawn_entities(mut commands: Commands) {
    commands.spawn(InfiniteGridBundle {
        settings: InfiniteGridSettings {
            // fadeout_distance: 400.0,
            ..Default::default()
        },
        ..Default::default()
    });
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
