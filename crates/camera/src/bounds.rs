use crate::OrbitState;
use bevy::prelude::*;

/// Local-space axis-aligned bounding box registered on an entity.
///
/// Inserting this component opts the entity into the camera-fit system.
/// Update it whenever the underlying geometry changes (e.g. when the LOD root
/// node of a point cloud becomes available, or when a mesh asset finishes
/// loading).
#[derive(Component, Clone, Debug)]
pub struct FitExtent {
    /// Local-space centre of the bounding box.
    pub center: Vec3,
    /// Local-space half-extents (radius along each axis).
    pub half_extents: Vec3,
}

impl FitExtent {
    /// Construct bounds from their local center and half extents.
    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            center,
            half_extents,
        }
    }

    /// Convenience: axis-aligned bounds defined by a min and max corner.
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        Self {
            center: (min + max) * 0.5,
            half_extents: (max - min) * 0.5,
        }
    }

    /// Compute a conservative world-space bounding sphere given a transform.
    ///
    /// Returns `(world_center, world_radius)`.
    pub fn world_sphere(&self, global_tf: &GlobalTransform) -> (Vec3, f32) {
        let world_center = global_tf.transform_point(self.center);
        // Diagonal of the OBB in local space (conservative sphere).
        let local_radius = self.half_extents.length();
        // Scale by the largest scale axis to stay conservative under non-uniform scale.
        let (scale, _, _) = global_tf.to_scale_rotation_translation();
        let world_radius = local_radius * scale.abs().max_element();
        (world_center, world_radius)
    }

    /// Return all eight corners transformed into world space.
    pub fn world_corners(&self, transform: &GlobalTransform) -> [Vec3; 8] {
        std::array::from_fn(|i| {
            let sign = Vec3::new(
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            );
            transform.transform_point(self.center + self.half_extents * sign)
        })
    }
}

/// Union actual transformed corners, not the boxes enclosing their spheres.
pub fn scene_bounding_sphere<'a>(
    extents: impl IntoIterator<Item = (&'a FitExtent, &'a GlobalTransform)>,
) -> Option<(Vec3, f32)> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (extent, transform) in extents {
        let corners = extent.world_corners(transform);
        if !corners.iter().all(|corner| corner.is_finite()) {
            continue;
        }
        for corner in corners {
            min = min.min(corner);
            max = max.max(corner);
        }
    }
    (min.is_finite() && max.is_finite()).then(|| ((min + max) * 0.5, (max - min).length() * 0.5))
}

const FIT_PADDING: f32 = 1.05;

/// Stable at every orbit angle. The narrower of the horizontal/vertical FOV
/// limits the distance; a small radial margin keeps the geometry off the edge.
pub fn perspective_fit_distance(radius: f32, fov: f32, viewport: Vec2) -> f32 {
    let aspect = if viewport.is_finite() && viewport.min_element() > 0.0 {
        viewport.x / viewport.y
    } else {
        1.0
    };
    let fov = crate::clamp_vertical_fov(fov);
    let half_vertical = fov * 0.5;
    let half_horizontal = (half_vertical.tan() * aspect).atan();
    (radius * FIT_PADDING / half_vertical.min(half_horizontal).sin()).max(0.05)
}

/// Request a perspective or orthographic fit without moving the camera directly.
pub fn fit_orbit(orbit: &mut OrbitState, center: Vec3, radius: f32, viewport: Vec2) {
    orbit.request_view();
    orbit.target_focus = center;
    orbit.target_distance = perspective_fit_distance(radius, orbit.target_fov, viewport);
    if orbit.use_ortho && viewport.is_finite() && viewport.min_element() > 0.0 {
        orbit.target_ortho_scale = (radius * 2.0 * FIT_PADDING / viewport.min_element()).max(1e-5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_distance_uses_the_limiting_fov_with_a_small_margin() {
        let square =
            perspective_fit_distance(1.0, std::f32::consts::FRAC_PI_2, Vec2::splat(1000.0));
        // 1.05 / sin(45 degrees).
        assert!((square - 1.4849242).abs() < 1e-5);
        let portrait =
            perspective_fit_distance(1.0, std::f32::consts::FRAC_PI_2, Vec2::new(500.0, 1000.0));
        // 1.05 / sin(atan(0.5)).
        assert!((portrait - 2.3478713).abs() < 1e-5);
        for fov in [0.0, std::f32::consts::PI, f32::NAN] {
            assert!(perspective_fit_distance(1.0, fov, Vec2::ZERO).is_finite());
        }
    }

    #[test]
    fn bounds_union_handles_translated_mirrored_extents_and_skips_invalid_data() {
        let extent = FitExtent::from_center_half_extents(Vec3::ZERO, Vec3::ONE);
        let left = GlobalTransform::from(
            Transform::from_xyz(-2.0, 0.0, 0.0).with_scale(Vec3::splat(-1.0)),
        );
        let right = GlobalTransform::from_translation(Vec3::new(2.0, 0.0, 0.0));
        let invalid = GlobalTransform::from_translation(Vec3::splat(f32::NAN));
        let (center, radius) =
            scene_bounding_sphere([(&extent, &left), (&extent, &right), (&extent, &invalid)])
                .expect("finite union");
        assert_eq!(center, Vec3::ZERO);
        assert!((radius - 11.0_f32.sqrt()).abs() < 1e-5);
        assert!(scene_bounding_sphere([(&extent, &invalid)]).is_none());
    }
}
