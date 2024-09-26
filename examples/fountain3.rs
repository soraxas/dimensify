use std::{collections::HashMap, hash::Hash};

use dimensify::Dimensify;
use rapier3d::prelude::*;

const MAX_NUMBER_OF_BODIES: usize = 400;

pub fn init_world(viewer: &mut Dimensify) {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let impulse_joints = ImpulseJointSet::new();
    let multibody_joints = MultibodyJointSet::new();

    let rad = 0.5;

    /*
     * Ground
     */
    let ground_size = 100.1;
    let ground_height = 2.1; // 16.0;

    let rigid_body = RigidBodyBuilder::fixed().translation(vector![0.0, -ground_height, 0.0]);
    let handle = bodies.insert(rigid_body);
    let collider = ColliderBuilder::cuboid(ground_size, ground_height, ground_size);
    colliders.insert_with_parent(collider, handle, &mut bodies);

    let mut body_handle_to_scene_handle = HashMap::new();

    // Callback that will be executed on the main loop to handle proximities.
    viewer.add_callback(move |mut graphics, physics, _, run_state| {
        let rigid_body = RigidBodyBuilder::dynamic().translation(vector![0.0, 10.0, 0.0]);
        let handle = physics.bodies.insert(rigid_body);
        let collider = match run_state.timestep_id % 3 {
            0 => ColliderBuilder::round_cylinder(rad, rad, rad / 10.0),
            1 => ColliderBuilder::cone(rad, rad),
            _ => ColliderBuilder::cuboid(rad, rad, rad),
        };

        physics
            .colliders
            .insert_with_parent(collider, handle, &mut physics.bodies);

        if let Some(graphics) = &mut graphics {
            let shandle = graphics.add_body(handle, &physics.bodies, &physics.colliders);
            body_handle_to_scene_handle.insert(handle, shandle);
        }

        if physics.bodies.len() > MAX_NUMBER_OF_BODIES {
            let mut to_remove: Vec<_> = physics
                .bodies
                .iter()
                .filter(|e| e.1.is_dynamic())
                .map(|e| (e.0, e.1.position().translation.vector))
                .collect();

            to_remove.sort_by(|a, b| {
                (a.1.x.abs() + a.1.z.abs())
                    .partial_cmp(&(b.1.x.abs() + b.1.z.abs()))
                    .unwrap()
                    .reverse()
            });

            let num_to_remove = to_remove.len() - MAX_NUMBER_OF_BODIES;
            for (handle, _) in &to_remove[..num_to_remove] {
                physics.bodies.remove(
                    *handle,
                    &mut physics.islands,
                    &mut physics.colliders,
                    &mut physics.impulse_joints,
                    &mut physics.multibody_joints,
                    true,
                );

                if let Some(graphics) = &mut graphics {
                    if let Some(shandle) = body_handle_to_scene_handle.remove(handle) {
                        graphics
                            .graphics
                            .remove_and_despawn_object_part(graphics.commands, shandle);
                    }
                }
            }
        }
    });

    /*
     * Set up the viewer.
     */
    viewer.set_world(bodies, colliders, impulse_joints, multibody_joints);
    // viewer
    //     .physics_state_mut()
    //     .integration_parameters
    //     .erp = 0.2;
    viewer.look_at(point![-30.0, 4.0, -30.0], point![0.0, 1.0, 0.0]);
}
