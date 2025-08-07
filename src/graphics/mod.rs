use bevy::prelude::*;
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin, InfiniteGridSettings};

#[cfg(feature = "physics")]
pub mod helpers;

/// prefab assets are currently dependent on rapier3d's shape
/// (which isn't necessarily but for convenience)
/// So cannot use prefab assets without physics
#[cfg(feature = "physics")]
pub mod prefab_assets;

pub fn plugin(app: &mut App) {
    // a.initialise_if_empty(meshes);

    #[cfg(feature = "physics")]
    app.add_systems(Startup, initialise_prefab_assets);
}

pub fn infinite_grid_plugin(app: &mut App) {
    fn spawn_grid(mut commands: Commands) {
        commands.spawn(InfiniteGridBundle {
            settings: InfiniteGridSettings {
                // fadeout_distance: 400.0,
                ..Default::default()
            },
            ..Default::default()
        });
    }

    app.add_plugins(InfiniteGridPlugin)
        .add_systems(Startup, spawn_grid);
}

#[cfg(feature = "physics")]
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
