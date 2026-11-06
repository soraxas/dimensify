use bevy::prelude::*;
use bevy_infinite_grid::{InfiniteGridBundle, InfiniteGridPlugin};

// #[cfg(feature = "physics")]
// pub mod rapier_helpers;

/// prefab assets are currently dependent on rapier3d's shape
/// (which isn't necessarily but for convenience)
/// So cannot use prefab assets without physics
// #[cfg(feature = "physics")]
pub mod prefab_assets;

use serde::{Deserialize, Serialize};

// TODO: add supported mode (2d, 3d, both)
// #[cfg(feature = "bevy")]
// fn supports<'a>(
//     self,
//     e: &mut bevy::prelude::EntityCommands<'a>,
// ) -> bevy::prelude::EntityCommands<'a> {
//     self.insert_into(e)
// }

pub use bevy::math::primitives::{
    Capsule3d,
    Cone,
    ConicalFrustum,
    Cuboid,
    Cylinder,
    Plane3d,
    Polyline3d,
    Segment3d,
    // 3d primitives
    Sphere,
    Tetrahedron,
    Torus,
    Triangle3d,
};

// TODO: add supported mode (2d, 3d, both)
// #[cfg(feature = "bevy")]
// fn supports<'a>(
//     self,
//     e: &mut bevy::prelude::EntityCommands<'a>,
// ) -> bevy::prelude::EntityCommands<'a> {
//     self.insert_into(e)
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Material {
    Color { r: f32, g: f32, b: f32, a: f32 },
}
pub enum Shape3d {
    Sphere(Sphere),
    Plane3d(Plane3d),
    // InfinitePlane3d(bm3d::InfinitePlane3d),
    // Line3d(bm3d::Line3d),
    Segment3d(Segment3d),
    Polyline3d(Polyline3d),
    Cuboid(Cuboid),
    Cylinder(Cylinder),
    Capsule3d(Capsule3d),
    Cone(Cone),
    ConicalFrustum(ConicalFrustum),
    Torus(Torus),
    Triangle3d(Triangle3d),
    Tetrahedron(Tetrahedron),
}

pub fn plugin(app: &mut App) {
    // a.initialise_if_empty(meshes);

    // #[cfg(feature = "physics")]
    app.add_systems(Startup, initialise_prefab_assets);
}

pub fn infinite_grid_plugin(app: &mut App) {
    fn spawn_grid(mut commands: Commands) {
        commands.spawn(InfiniteGridBundle::default());
    }

    app.add_plugins(InfiniteGridPlugin)
        .add_systems(Startup, spawn_grid);
}

// #[cfg(feature = "physics")]
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
