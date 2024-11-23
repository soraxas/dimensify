use bevy::math::{NormedVectorSpace, Ray3d, Vec3};

/// Function to calculate ray intersection
pub fn ray_intersection(ray1: Ray3d, ray2: Ray3d, enforce_positive_dir: bool) -> Option<Vec3> {
    let cross_product = ray1.direction.cross(*ray2.direction);

    // If the directions are parallel (cross product is zero), the rays do not intersected_line
    let cross_product_norm_squared = cross_product.norm_squared();
    if cross_product_norm_squared < 1e-6 {
        return None;
    }

    let origin_diff = ray2.origin - ray1.origin;

    // Calculate the parameters t and s that minimize the distance
    let t = origin_diff.cross(*ray2.direction).dot(cross_product) / cross_product_norm_squared;
    let s = origin_diff.cross(*ray1.direction).dot(cross_product) / cross_product_norm_squared;

    if enforce_positive_dir && (t < 0.0 || s < 0.0) {
        return None;
    }

    // Calculate the closest points on the two rays
    let closest_point_on_ray1 = ray1.origin + t * ray1.direction; // Convert UnitVector to Vector3
    let closest_point_on_ray2 = ray2.origin + s * ray2.direction; // Convert UnitVector to Vector3

    // The intersection point is the average of the closest points on both rays
    Some((closest_point_on_ray1 + closest_point_on_ray2) / 2.0)
}
