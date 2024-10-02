use dimensify::Dimensify;
use rapier3d::prelude::*;

pub fn init_world(viewer: &mut Dimensify) {
    /*
     * World
     */
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let impulse_joints = ImpulseJointSet::new();
    let multibody_joints = MultibodyJointSet::new();

    /*
     * Ground
     */
    let ground_size = 100.1;
    let ground_height = 2.1;

    let rigid_body = RigidBodyBuilder::fixed().translation(vector![0.0, -ground_height, 0.0]);
    let handle = bodies.insert(rigid_body);
    let collider = ColliderBuilder::cuboid(ground_size, ground_height, ground_size);
    colliders.insert_with_parent(collider, handle, &mut bodies);

    /*
     * Create the primitives
     */
    let num = 8;
    let rad = 1.0;

    let shiftx = rad * 2.0 + rad;
    let shifty = rad * 2.0 + rad;
    let shiftz = rad * 2.0 + rad;
    let centerx = shiftx * (num / 2) as f32;
    let centery = shifty / 2.0;
    let centerz = shiftz * (num / 2) as f32;

    let mut offset = -(num as f32) * (rad * 2.0 + rad) * 0.5;

    for j in 0usize..20 {
        for i in 0..num {
            for k in 0usize..num {
                let x = i as f32 * shiftx - centerx + offset;
                let y = j as f32 * shifty + centery + 3.0;
                let z = k as f32 * shiftz - centerz + offset;

                // Build the rigid body.
                let rigid_body = RigidBodyBuilder::dynamic().translation(vector![x, y, z]);
                let handle = bodies.insert(rigid_body);

                let collider = match j % 5 {
                    // _ => ColliderBuilder::cuboid(rad, rad, rad),
                    1 => ColliderBuilder::ball(rad),
                    // Rounded cylinders are much more efficient that cylinder, even if the
                    // rounding margin is small.
                    2 => ColliderBuilder::round_cylinder(rad, rad, rad / 10.0),
                    3 => ColliderBuilder::cone(rad, rad),
                    _ => ColliderBuilder::capsule_y(rad, rad),
                };

                colliders.insert_with_parent(collider, handle, &mut bodies);
            }
        }

        offset -= 0.05 * rad * (num as f32 - 1.0);
    }

    /*
     * Set up the viewer.
     */
    viewer.set_world(bodies, colliders, impulse_joints, multibody_joints);
    viewer.look_at(point![100.0, 100.0, 100.0], Point::origin());
}
