#![allow(dead_code)]

use std::collections::HashMap;

use dimensify::scene_graphics::entity_spawner::spawn_from_datapack::ColliderDataType;
use dimensify::scene_graphics::entity_spawner::{builder_from_collider_builder, EntitySpawnerArg};
use dimensify::scene_graphics::entity_spawner::{spawn_from_datapack, EntitySpawner};
use dimensify::{Dimensify, DimensifyApp};
use rapier3d::prelude::*;

pub fn init_world(viewer: &mut Dimensify) {
    if let Some(graphics) = viewer.graphics.as_mut() {
        graphics
            .graphics
            .pending_entity_spawners
            .push(Box::new(|args: EntitySpawnerArg| {
                let EntitySpawnerArg {
                    commands,
                    meshes,
                    materials,
                    bodies,
                    colliders,
                    prefab_meshes,
                    instanced_materials,
                    ..
                } = args;
                let mut entities: HashMap<RigidBodyHandle, Vec<_>> = HashMap::new();
                /*
                 * Ground
                 */
                let ground_size = 5.0;
                let ground_height = 0.1;

                let rigid_body =
                    RigidBodyBuilder::fixed().translation(vector![0.0, -ground_height, 0.0]);
                let floor_handle = bodies.insert(rigid_body);

                let mut datapacks = Vec::new();

                datapacks.push(
                    spawn_from_datapack::EntityDataBuilder::default()
                        .collider(Some(
                            ColliderBuilder::cuboid(ground_size, ground_height, ground_size).into(),
                        ))
                        .body(Some(floor_handle.into()))
                        .done(),
                );

                datapacks.push(
                    spawn_from_datapack::EntityDataBuilder::default()
                        .collider(Some(ColliderBuilder::ball(5.).into()))
                        .material([0.1, 0.5, 0.9].into())
                        .node_pos(Some(point![0., 9., 0.].into()))
                        .body(Some(floor_handle.into()))
                        .done(),
                );

                datapacks.push(
                    spawn_from_datapack::EntityDataBuilder::default()
                        .collider(Some(ColliderBuilder::capsule_z(2., 3.).into()))
                        .material([0.3, 0.5, 0.9].into())
                        .node_pos(Some(point![4., 9., 0.].into()))
                        .body(Some(RigidBodyBuilder::dynamic().build().into()))
                        .done(),
                );

                for datapack in datapacks {
                    // for (handle, datapack) in datapacks {
                    // entities.entry(handle).or_default().push(
                    spawn_from_datapack::spawn_datapack(
                        commands,
                        meshes,
                        materials,
                        datapack,
                        Some(prefab_meshes),
                        Some(colliders),
                        Some(bodies),
                    )
                    .expect("all fields are set");

                    // );
                }

                entities
            }));
    }

    /*
     * Set up the viewer.
     */
    viewer.clear();
    viewer.reset_graphics();

    viewer.look_at(point![100.0, 100.0, 100.0], Point::origin());
}

// #[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn main() {
    let mut builders: Vec<(_, fn(&mut Dimensify))> = vec![("Hello World", init_world)];

    let i = 0;
    let viewer = DimensifyApp::from_builders(i, builders);
    viewer.run()
}
