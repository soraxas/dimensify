//! Headless semantic Fly input: no window, renderer, robot or physics plugin.
use bevy::prelude::*;
use dimensify_camera::{CameraPose, FlyInput, PoseTransition, fly_velocity, smooth_look};

fn main() {
    let mut pose = CameraPose::from_rotation(Vec3::ZERO, Quat::IDENTITY, 5.0, 0.8);
    let mut angles = Vec2::ZERO;
    let mut look_target = None;
    let mut momentum = Vec3::ZERO;
    let mut transition = PoseTransition::default();
    let dt = 1.0 / 60.0;
    for frame in 0..240 {
        let delta = if frame == 0 {
            Vec2::new(0.5, 0.2)
        } else {
            Vec2::ZERO
        };
        angles = smooth_look(angles, &mut look_target, delta, dt);
        let rotation = Quat::from_rotation_y(angles.x) * Quat::from_rotation_x(angles.y);
        let input = FlyInput {
            move_local: if frame < 60 { Vec3::Z } else { Vec3::ZERO },
            ..default()
        };
        let velocity = fly_velocity(&mut momentum, rotation, angles.x, &input, dt);
        // A collision adapter may constrain this proposed displacement.
        let target = CameraPose::from_rotation(
            pose.position + velocity * dt,
            rotation,
            pose.distance,
            pose.fov,
        );
        // Only this consumer writes the live pose and its eventual transform.
        pose = transition.advance(&target, dt);
    }
    assert_eq!(momentum, Vec3::ZERO);
    println!("Final eye: {:?}; motion settled", pose.position);
}
