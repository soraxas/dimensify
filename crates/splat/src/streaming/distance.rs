//! Full-bound distance mode from deployed PlayCanvas dfcc50f's
//! GSplatOctreeInstance.evaluateNodeCoverage. The viewer's behind penalty is 5.
//! MIT license, Copyright (c) 2011-2026 PlayCanvas Ltd.
//! See docs/legal/playcanvas-MIT.txt for the full notice.

use super::manifest::Bounds;
use bevy::prelude::*;

/// The deployed viewer uses the full authored box, without the oversized-axis
/// shrink found in the local a957ec2 research checkout. Use this consistently
/// for desired detail, resident upgrades and whole-file relocation.
pub fn full_bounds_squared(bound: Bounds, eye: Vec3, forward: Vec3) -> f64 {
    let delta = eye.clamp(bound.min, bound.max) - eye;
    let squared = f64::from(delta.length_squared());
    let behind = if squared > 0.0001 {
        f64::from((-delta.dot(forward)).max(0.0)) / squared.sqrt()
    } else {
        0.0
    };
    squared * (1.0 + 4.0 * behind).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_box_coverage_does_not_shrink_large_leaves_away_from_the_eye() {
        let bound = Bounds {
            min: Vec3::new(-100., -1., -1.),
            max: Vec3::new(100., 1., 1.),
        };
        assert_eq!(
            full_bounds_squared(bound, Vec3::new(90., 0., 0.), Vec3::NEG_Z),
            0.
        );
        assert_eq!(
            full_bounds_squared(bound, Vec3::new(102., 0., 0.), Vec3::NEG_X),
            4.
        );
    }

    #[test]
    fn behind_penalty_uses_nearest_box_point_not_its_center() {
        let bound = Bounds {
            min: -Vec3::ONE,
            max: Vec3::ONE,
        };
        let eye = Vec3::new(-3., 0., 0.);
        assert_eq!(full_bounds_squared(bound, eye, Vec3::X), 4.);
        assert_eq!(full_bounds_squared(bound, eye, Vec3::Y), 4.);
        assert_eq!(full_bounds_squared(bound, eye, Vec3::NEG_X), 100.);
    }

    #[test]
    fn near_zero_distances_skip_directional_division() {
        let bound = Bounds {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
        };
        assert_eq!(full_bounds_squared(bound, Vec3::ZERO, Vec3::NEG_Z), 0.);
        let eye = Vec3::new(0., 0., 0.001);
        assert_eq!(
            full_bounds_squared(bound, eye, Vec3::Z),
            f64::from(eye.length_squared())
        );
    }
}
