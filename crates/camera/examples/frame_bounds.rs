//! Fit a scene without opening a window or initializing a renderer.
use bevy::prelude::*;
use dimensify_camera::{FitExtent, OrbitState, fit_orbit, scene_bounding_sphere};

fn main() {
    let extent = FitExtent::from_min_max(-Vec3::ONE, Vec3::ONE);
    let transform = GlobalTransform::from_translation(Vec3::new(2., 0., 0.));
    let (center, radius) = scene_bounding_sphere([(&extent, &transform)]).unwrap();
    let mut view = OrbitState::default();
    fit_orbit(&mut view, center, radius, Vec2::new(1200., 800.));
    let pose = view.requested_pose();
    println!(
        "Eye: {:?}; focus: {:?}; distance: {}",
        pose.position,
        pose.focus(),
        pose.distance
    );
}
