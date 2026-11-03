use bevy::prelude::*;
use dimensify_camera::{CameraPath, CameraPose};
use std::time::Duration;

fn start() -> CameraPose {
    CameraPose::looking(Vec3::new(2., 1., 6.), Vec3::ZERO, 0.8)
}
fn matches(a: CameraPose, b: CameraPose) {
    assert!(a.position.abs_diff_eq(b.position, 1e-5));
    assert!(a.rotation().dot(b.rotation()).abs() > 1. - 1e-6);
    assert!((a.fov - b.fov).abs() < 1e-6);
}
#[test]
fn timed_path_is_independent_of_frame_history_and_holds_exact_endpoints() {
    let from = start();
    let to = CameraPose::looking(Vec3::new(-3., 2., 4.), Vec3::ZERO, 0.6);
    let path = CameraPath::between(from, to, Duration::from_secs(2)).unwrap();
    matches(path.sample(Duration::ZERO), from);
    matches(path.sample(Duration::from_secs(99)), to);
    for fps in [30, 60, 120] {
        for frame in 0..fps {
            std::hint::black_box(path.sample(Duration::from_secs_f64(frame as f64 / fps as f64)));
        }
        matches(
            path.sample(Duration::from_secs(1)),
            path.sample(Duration::from_millis(1000)),
        );
    }
    // Random access and reverse seeking do not depend on prior frames.
    let expected = path.sample(Duration::from_millis(731));
    path.sample(Duration::from_millis(1900));
    matches(path.sample(Duration::from_millis(731)), expected);
}
#[test]
fn orbit_tour_returns_exactly_and_preserves_its_pivot_and_lens() {
    let from = start();
    let path = CameraPath::orbit(from, from.focus(), 0.8, Duration::from_secs(4)).unwrap();
    for ms in (0..4000).step_by(17) {
        let pose = path.sample(Duration::from_millis(ms));
        assert!(pose.focus().distance(from.focus()) < 1e-4);
        assert!((pose.position.distance(from.focus()) - from.distance).abs() < 1e-4);
        assert_eq!(pose.fov, from.fov);
    }
    assert!(
        path.sample(Duration::from_secs(2))
            .position
            .distance(from.position)
            > 1.
    );
    matches(path.sample(Duration::from_secs(4)), from);
}
#[test]
fn movement_eases_in_and_out_without_overshooting() {
    let from = start();
    let to = CameraPose::from_rotation(
        from.position + Vec3::X,
        from.rotation(),
        from.distance,
        from.fov,
    );
    let path = CameraPath::between(from, to, Duration::from_secs(1)).unwrap();
    let first = path
        .sample(Duration::from_millis(1))
        .position
        .distance(from.position);
    let middle = path
        .sample(Duration::from_millis(501))
        .position
        .distance(path.sample(Duration::from_millis(500)).position);
    assert!(first < middle * 0.01);
    for ms in 0..=1000 {
        let x = path.sample(Duration::from_millis(ms)).position.x;
        assert!(x >= from.position.x && x <= to.position.x);
    }
}
#[test]
fn invalid_camera_paths_are_rejected() {
    let from = start();
    assert!(CameraPath::between(from, from, Duration::ZERO).is_err());
    assert!(CameraPath::orbit(from, Vec3::NAN, 1., Duration::from_secs(1)).is_err());
    assert!(CameraPath::orbit(from, Vec3::ZERO, f32::NAN, Duration::from_secs(1)).is_err());
    let mut invalid = from;
    invalid.position.x = f32::INFINITY;
    assert!(CameraPath::between(invalid, from, Duration::from_secs(1)).is_err());
}

#[test]
fn easing_roundoff_never_moves_beyond_the_endpoint() {
    let from = CameraPose::from_rotation(Vec3::ZERO, Quat::IDENTITY, 1., 0.8);
    let to = CameraPose {
        position: Vec3::X * 100.,
        ..from
    };
    let path = CameraPath::between(from, to, Duration::from_secs(1)).unwrap();
    for micros in 999000..1000000 {
        assert!(path.sample(Duration::from_micros(micros)).position.x <= 100.);
    }
}

#[test]
fn very_small_positive_lens_and_distance_remain_positive_near_completion() {
    let from = start();
    let to = CameraPose {
        distance: 1e-12,
        fov: 1e-12,
        ..from
    };
    let path = CameraPath::between(from, to, Duration::from_secs(1)).unwrap();
    for micros in 999000..1000000 {
        let pose = path.sample(Duration::from_micros(micros));
        assert!(pose.distance > 0. && pose.fov > 0.);
    }
}
