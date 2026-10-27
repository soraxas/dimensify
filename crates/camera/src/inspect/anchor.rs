use crate::CameraPose;
use bevy::{math::DVec3, prelude::*};

/// Intersect a ray interval with an axis-aligned clipping box.
pub fn clip_ray_interval(
    origin: Vec3,
    direction: Vec3,
    min: Vec3,
    max: Vec3,
    mut interval: (f32, f32),
) -> Option<(f32, f32)> {
    for axis in 0..3 {
        if direction[axis].abs() < 1e-8 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            interval.0 = interval.0.max(a.min(b));
            interval.1 = interval.1.min(a.max(b));
            if interval.0 > interval.1 {
                return None;
            }
        }
    }
    Some(interval)
}

/// Pick a finite view-space anchor from a surface or pointer focus plane.
pub fn inspect_view_anchor(
    camera: &Camera,
    pose: &CameraPose,
    pointer: Option<Vec2>,
    surface: Option<Vec3>,
    last_depth: f64,
) -> DVec3 {
    let inverse = pose.rotation().inverse();
    if let Some(point) = surface {
        let view = inverse * (point - pose.position);
        if view.is_finite() && view.z < -0.001 {
            return view.as_dvec3();
        }
    }
    let depth = if pose.distance.is_finite() && pose.distance > 0.001 {
        pose.distance as f64
    } else {
        last_depth.abs().max(0.01)
    };
    // Intersect the pointer ray with the view-space plane z = -depth. This
    // works for both perspective and orthographic rays, including viewport offsets.
    if let Some(ray) = pointer.and_then(|p| {
        camera
            .viewport_to_world(&GlobalTransform::from(pose.to_transform()), p)
            .ok()
    }) {
        let origin = (inverse * (ray.origin - pose.position)).as_dvec3();
        let direction = (inverse * *ray.direction).as_dvec3();
        if direction.z < -1e-6 {
            let view = origin + direction * ((-depth - origin.z) / direction.z);
            if view.is_finite() {
                return view;
            }
        }
    }
    DVec3::new(0.0, 0.0, -depth)
}
