use std::collections::HashMap;

use dimensify::scene_graphics::entity_spawner::EntitySpawner;
use dimensify::scene_graphics::entity_spawner::{ColliderAsMeshSpawner, EntitySpawnerArg};
use dimensify::Dimensify;
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

                entities.entry(floor_handle).or_default().push(
                    ColliderAsMeshSpawner::builder_from_collider_builder(
                        ColliderBuilder::cuboid(ground_size, ground_height, ground_size),
                        floor_handle,
                        colliders,
                        bodies,
                        prefab_meshes,
                        instanced_materials,
                    )
                    .build()
                    .unwrap()
                    .spawn(commands, meshes, materials),
                );

                /*
                 * Setup groups
                 */
                const GREEN_GROUP: InteractionGroups =
                    InteractionGroups::new(Group::GROUP_1, Group::GROUP_1);
                const BLUE_GROUP: InteractionGroups =
                    InteractionGroups::new(Group::GROUP_2, Group::GROUP_2);

                /*
                 * A green floor that will collide with the GREEN group only.
                 */
                let green_floor = ColliderBuilder::cuboid(1.0, 0.1, 1.0)
                    .translation(vector![0.0, 1.0, 0.0])
                    .collision_groups(GREEN_GROUP);

                entities.entry(floor_handle).or_default().push(
                    ColliderAsMeshSpawner::builder_from_collider_builder(
                        green_floor,
                        floor_handle,
                        colliders,
                        bodies,
                        prefab_meshes,
                        instanced_materials,
                    )
                    .color([0.0, 1.0, 0.0].into())
                    .build()
                    .unwrap()
                    .spawn(commands, meshes, materials),
                );

                /*
                 * A blue floor that will collide with the BLUE group only.
                 */
                let blue_floor = ColliderBuilder::cuboid(1.0, 0.1, 1.0)
                    .translation(vector![0.0, 2.0, 0.0])
                    .collision_groups(BLUE_GROUP);

                entities.entry(floor_handle).or_default().push(
                    ColliderAsMeshSpawner::builder_from_collider_builder(
                        blue_floor,
                        floor_handle,
                        colliders,
                        bodies,
                        prefab_meshes,
                        instanced_materials,
                    )
                    .color([0.0, 0.0, 1.0].into())
                    .build()
                    .unwrap()
                    .spawn(commands, meshes, materials),
                );

                /*
                 * Create the cubes
                 */
                let num = 8;
                let rad = 0.1;

                let shift = rad * 2.0;
                let centerx = shift * (num / 2) as f32;
                let centery = 2.5;
                let centerz = shift * (num / 2) as f32;

                for j in 0usize..4 {
                    for i in 0..num {
                        for k in 0usize..num {
                            let x = i as f32 * shift - centerx;
                            let y = j as f32 * shift + centery;
                            let z = k as f32 * shift - centerz;

                            // Alternate between the green and blue groups.
                            let (group, color) = if k % 2 == 0 {
                                (GREEN_GROUP, [0.0, 1.0, 0.0])
                            } else {
                                (BLUE_GROUP, [0.0, 0.0, 1.0])
                            };

                            let rigid_body =
                                RigidBodyBuilder::dynamic().translation(vector![x, y, z]);
                            let handle = bodies.insert(rigid_body);

                            entities.entry(handle).or_default().push(
                                ColliderAsMeshSpawner::builder_from_collider_builder(
                                    ColliderBuilder::cuboid(rad, rad, rad).collision_groups(group),
                                    handle,
                                    colliders,
                                    bodies,
                                    prefab_meshes,
                                    instanced_materials,
                                )
                                .color(color.into())
                                .build()
                                .unwrap()
                                .spawn(commands, meshes, materials),
                            );
                        }
                    }
                }

                entities
            }));
    }

    /*
     * Set up the viewer.
     */
    viewer.clear();
    viewer.reset_graphics();

    // viewer.set_world(bodies, colliders, impulse_joints, multibody_joints);
    viewer.look_at(point!(10.0, 10.0, 10.0), Point::origin());
}
