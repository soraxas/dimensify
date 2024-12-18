use bevy::math::{Quat, Vec3};

const EPSILON_AS_ZERO: f32 = 1e-6;

pub(crate) trait Vec3Ext: Copy {
    fn is_approx_zero(self) -> bool;
    fn to_horizontal(self) -> Vec3;
}

impl Vec3Ext for Vec3 {
    #[inline]
    fn is_approx_zero(self) -> bool {
        self.length_squared() < EPSILON_AS_ZERO
    }

    #[inline]
    fn to_horizontal(self) -> Vec3 {
        Vec3::new(self.x, 0., self.z)
    }
}

/// Trait to calculate the distance between two quaternions
pub trait BevyQuatDistanceTrait {
    fn distance(&self, other: Self) -> f32;
}

impl BevyQuatDistanceTrait for Quat {
    fn distance(&self, other: Self) -> f32 {
        // Normalize the quaternions (in case they are not unit quaternions)
        let q1 = self.normalize();
        let q2 = other.normalize();

        // Compute the dot product of the quaternions
        let dot = q1.dot(q2);

        // Clamp the dot product to avoid NaN due to floating point errors
        let dot = dot.clamp(-1.0, 1.0);

        // Calculate the angular distance (in radians)
        2.0 * dot.abs().acos()
    }
}
