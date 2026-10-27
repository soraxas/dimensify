use bevy::prelude::*;
use dimensify_camera::{CameraPose, FlyInput, PoseTransition, fly_velocity, smooth_look};

#[test]
fn mode_transition_moves_the_eye_directly_and_finishes_at_the_current_target() {
    let start = CameraPose::from_rotation(Vec3::ZERO, Quat::IDENTITY, 100.0, 0.8);
    let mut target = CameraPose::from_rotation(Vec3::X * 8.0, Quat::IDENTITY, 1.0, 1.2);
    let mut transition = PoseTransition::default();
    transition.start(&start);
    let halfway = transition.advance(&target, 0.5);
    assert!(halfway.position.abs_diff_eq(Vec3::X * 7.0, 1e-6));
    assert!((halfway.fov - 1.15).abs() < 1e-6);
    target.position = Vec3::Y * 3.0;
    let settled = transition.advance(&target, 0.5);
    assert_eq!(settled.position, target.position);
    assert_eq!(transition.advance(&start, 0.1).position, start.position);
}

#[test]
fn retargeting_a_transition_starts_at_the_visible_pose() {
    let start = CameraPose::from_rotation(Vec3::ZERO, Quat::IDENTITY, 1.0, 0.8);
    let target = CameraPose::from_rotation(Vec3::X * 8.0, Quat::IDENTITY, 1.0, 0.8);
    let mut transition = PoseTransition::default();
    transition.start(&start);
    let live = transition.advance(&target, 0.5);
    transition.start(&live);
    assert_eq!(transition.advance(&start, 0.0).position, Vec3::X * 7.0);
    assert_eq!(transition.advance(&start, 1.0).position, Vec3::ZERO);
}

#[test]
fn look_takes_the_short_yaw_path_and_retains_a_finite_smoothing_tail() {
    let mut target = None;
    let mut angles = Vec2::new(359_f32.to_radians(), 0.0);
    angles = smooth_look(
        angles,
        &mut target,
        Vec2::new(2_f32.to_radians(), 4.0),
        0.01,
    );
    assert!(angles.x > 359_f32.to_radians());
    assert!(angles.y > 0.0 && angles.y < std::f32::consts::FRAC_PI_2);
    for _ in 0..100 {
        angles = smooth_look(angles, &mut target, Vec2::ZERO, 0.01);
    }
    assert!((angles.x - 1_f32.to_radians()).abs() < 1e-5);
    assert!((angles.y - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
}

#[test]
fn fly_pointer_impulse_is_independent_of_frame_rate_and_keyboard_speed() {
    for dt in [1.0 / 30.0, 1.0 / 60.0, 1.0 / 120.0] {
        for speed in [1.0, 4.0, 16.0] {
            let mut momentum = Vec3::ZERO;
            let input = FlyInput {
                pointer_move: Vec3::new(0.2, 0.3, 0.4),
                speed,
                ..default()
            };
            let velocity = fly_velocity(&mut momentum, Quat::IDENTITY, 0.0, &input, dt);
            assert!((velocity * dt).abs_diff_eq(Vec3::new(0.2, 0.3, -0.4), 1e-6));
            assert_eq!(momentum, Vec3::ZERO);
            assert_eq!(
                fly_velocity(&mut momentum, Quat::IDENTITY, 0.0, &FlyInput::default(), dt),
                Vec3::ZERO
            );
        }
    }
}

#[test]
fn fly_keyboard_normalizes_diagonals_and_drains_to_idle() {
    let mut momentum = Vec3::ZERO;
    let input = FlyInput {
        move_local: Vec3::ONE,
        ..default()
    };
    for _ in 0..120 {
        fly_velocity(&mut momentum, Quat::IDENTITY, 0.0, &input, 1.0 / 60.0);
    }
    assert!((momentum.length() - 4.0).abs() < 1e-5);
    for _ in 0..240 {
        fly_velocity(
            &mut momentum,
            Quat::IDENTITY,
            0.0,
            &FlyInput::default(),
            1.0 / 60.0,
        );
    }
    assert_eq!(momentum, Vec3::ZERO);
}

#[test]
fn planar_fly_keeps_keyboard_level_and_pointer_in_the_displayed_plane() {
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
        * Quat::from_rotation_x(std::f32::consts::FRAC_PI_4);
    let mut momentum = Vec3::ZERO;
    let input = FlyInput {
        move_local: Vec3::Z,
        pointer_move: Vec3::Y,
        along_view: false,
        ..default()
    };
    let velocity = fly_velocity(
        &mut momentum,
        rotation,
        std::f32::consts::FRAC_PI_2,
        &input,
        1.0,
    );
    assert!(velocity.abs_diff_eq(Vec3::new(-3.292_893_2, 0.707_106_77, 0.0), 0.002));
}
