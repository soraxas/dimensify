#![allow(dead_code)]

use dimensify::scene_graphics::entity_spawner::{builder_from_collider_builder, EntitySpawnerArg};
use dimensify::scene_graphics::entity_spawner::{spawn_from_datapack, EntitySpawner};
use dimensify::{Dimensify, DimensifyApp};
use rapier3d::prelude::*;

pub fn init_world(viewer: &mut Dimensify) {
    let mut bodies = RigidBodySet::new();

    /*
     * Ground
     */
    let ground_size = 5.0;
    let ground_height = 0.1;

    let rigid_body = RigidBodyBuilder::fixed().translation(vector![0.0, -ground_height, 0.0]);
    let floor_handle = bodies.insert(rigid_body);

    let datapacks = vec![
        spawn_from_datapack::EntityDataBuilder::default()
            .collider(Some(
                ColliderBuilder::cuboid(ground_size, ground_height, ground_size).into(),
            ))
            .body(Some(floor_handle.into()))
            .done(),
        spawn_from_datapack::EntityDataBuilder::default()
            .collider(Some(ColliderBuilder::ball(5.).into()))
            .material([0.1, 0.5, 0.99].into())
            .node_pos(Some(point![0., 9., 0.].into()))
            .body(Some(floor_handle.into()))
            .done(),
        spawn_from_datapack::EntityDataBuilder::default()
            .collider(Some(ColliderBuilder::capsule_z(2., 3.).into()))
            .material([0.3, 0.5, 0.9].into())
            .node_pos(Some(point![4., 9., 0.].into()))
            .body(Some(RigidBodyBuilder::dynamic().build().into()))
            .done(),
        spawn_from_datapack::EntityDataBuilder::default()
            .collider(Some(
                ColliderBuilder::capsule_z(2., 3.)
                    .translation(vector![0., 2., 0.])
                    .into(),
            ))
            .material([0.3, 0.2, 0.9].into())
            // .node_pos(Some(point![, 9., 0.].into()))
            .body(Some(RigidBodyBuilder::dynamic().build().into()))
            .done(),
    ];

    /*
     * Set up the viewer.
     */
    viewer.clear();
    viewer.reset_graphics();
    viewer.new_world_datapacks_with_sets(datapacks, Some(bodies), None);

    viewer.look_at(point![100.0, 100.0, 100.0], Point::origin());
}

// #[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    let mut builders: Vec<(_, fn(&mut Dimensify))> = vec![("Hello World", init_world)];

    let i = 0;
    let viewer = DimensifyApp::from_builders(i, builders);
    viewer.run()
}
