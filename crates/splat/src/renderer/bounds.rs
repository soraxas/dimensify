//! Shared extent-aware bounds for snapshot and persistent streamed clouds.
use bevy::{camera::primitives::Aabb, prelude::*};
use bevy_gaussian_splatting::PlanarGaussian3d;

/// The persistent adapter's existing three-scale sphere bound, shared with the
/// snapshot adapter so Bevy does not fall back to upstream's centre +/- 0.1 box.
/// Ignore alignment padding. GPU sources calculate this once per residency;
/// snapshots calculate it once per composition, not on every camera frame.
pub(super) fn cloud_bounds(cloud: &PlanarGaussian3d, count: usize) -> Result<Aabb, String> {
    if count == 0 || count > cloud.position_visibility.len() || count > cloud.scale_opacity.len() {
        return Err("invalid Gaussian bounds count".into());
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for i in 0..count {
        let p = Vec3::from_array(cloud.position_visibility[i].position);
        let radius = Vec3::from_array(cloud.scale_opacity[i].scale)
            .abs()
            .max_element()
            * 3.0;
        if !p.is_finite() || !radius.is_finite() {
            return Err("non-finite Gaussian source bounds".into());
        }
        min = min.min(p - Vec3::splat(radius));
        max = max.max(p + Vec3::splat(radius));
    }
    let result = Aabb::from_min_max(min, max);
    if !result.center.is_finite() || !result.half_extents.is_finite() {
        return Err("Gaussian source bounds overflow".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_gaussian_splatting::Gaussian3d;

    #[test]
    fn bounds_include_anisotropic_support_without_alignment_padding() {
        let mut splat = Gaussian3d::default();
        splat.position_visibility.position = [20., 2., -30.];
        splat.scale_opacity.scale = [0.1, 2., 0.3];
        let mut padding = Gaussian3d::default();
        padding.position_visibility.position = [-1000.; 3];
        let cloud: PlanarGaussian3d = vec![splat, padding].into();
        let bounds = cloud_bounds(&cloud, 1).expect("logical bounds");
        assert_eq!(Vec3::from(bounds.center), Vec3::new(20., 2., -30.));
        assert_eq!(Vec3::from(bounds.half_extents), Vec3::splat(6.));
        assert!(cloud_bounds(&cloud, 0).is_err());
        assert!(cloud_bounds(&cloud, 3).is_err());
    }
}
