/// These functions switch the handedness of a coordinate system.
/// As well as swapping the Y and Z axes.
use bevy::math::Mat4;
use bevy::math::Quat as BevyQuat;
use bevy::math::Vec3 as BevyVec3;
use bevy::math::Vec4 as BevyVec4;
use bevy::prelude::Transform as BevyTransform;

/// Trait for flipping the handiness of an object.
pub trait SwapYZTrait {
    #[must_use]
    fn swap_yz_axis(self) -> Self;
}

/// Trait for flipping the hand of an object.
pub trait FlipHandTrait {
    #[must_use]
    fn flip_hand(self) -> Self;
}

/// Trait for flipping the hand and swapping the Y and Z axes of an object.
pub trait SwapYZandFlipHandTrait {
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self;
}

////////////////////////////////////////////////////////////

impl SwapYZTrait for BevyVec3 {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis(self) -> Self {
        BevyVec3::new(self.x, self.z, self.y)
    }
}

impl SwapYZTrait for k::nalgebra::Translation3<f32> {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis(self) -> Self {
        k::nalgebra::Translation3::new(self.x, self.z, self.y)
    }
}

/////////////////////////

pub trait SwapYZandFlipHandTraitInverse {
    #[must_use]
    fn swap_yz_axis_and_flip_hand_inverse(self) -> Self;
}

impl SwapYZandFlipHandTraitInverse for BevyTransform {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand_inverse(self) -> Self {
        self.mul_transform(BevyTransform::from_matrix(Mat4 {
            x_axis: BevyVec4::new(1.0, 0.0, 0.0, 0.0), // X-axis remains the same
            y_axis: BevyVec4::new(0.0, 0.0, -1.0, 0.0), // Y-axis becomes Z
            z_axis: BevyVec4::new(0.0, 1.0, 0.0, 0.0), // Z-axis becomes Y
            w_axis: BevyVec4::new(0.0, 0.0, 0.0, 1.0), // Homogeneous component
        }))
    }
}

/////////////////////////

impl FlipHandTrait for BevyQuat {
    #[inline(always)]
    #[must_use]
    fn flip_hand(self) -> Self {
        BevyQuat::from_xyzw(self.x, self.y, -self.z, self.w)
    }
}

impl FlipHandTrait for BevyTransform {
    #[inline(always)]
    #[must_use]
    fn flip_hand(mut self) -> Self {
        self.rotation = self.rotation.flip_hand();
        self
    }
}

impl FlipHandTrait for k::nalgebra::Isometry3<f32> {
    #[inline(always)]
    fn flip_hand(mut self) -> Self {
        self.rotation = self.rotation.swap_yz_axis_and_flip_hand();
        self
    }
}

/////////////////////////

impl SwapYZandFlipHandTrait for BevyQuat {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self {
        BevyQuat::from_xyzw(self.x, -self.z, self.y, self.w)
    }
}

impl SwapYZandFlipHandTrait for BevyTransform {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self {
        self.mul_transform(BevyTransform::from_matrix(Mat4 {
            x_axis: BevyVec4::new(1.0, 0.0, 0.0, 0.0), // X-axis remains the same
            y_axis: BevyVec4::new(0.0, 0.0, 1.0, 0.0), // Y-axis becomes Z
            z_axis: BevyVec4::new(0.0, -1.0, 0.0, 0.0), // Z-axis becomes -Y
            w_axis: BevyVec4::new(0.0, 0.0, 0.0, 1.0), // Homogeneous component
        }))
    }
}

impl SwapYZandFlipHandTrait for k::nalgebra::UnitQuaternion<f32> {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self {
        self * k::nalgebra::UnitQuaternion::from_matrix(&k::nalgebra::Matrix3::new(
            1.0, 0.0, 0.0, // X-axis remains the same
            0.0, 0.0, 1.0, // Y-axis becomes Z
            0.0, -1.0, 0.0, // Z-axis becomes -Y
        ))
    }
}

impl SwapYZandFlipHandTrait for k::Isometry3<f32> {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self {
        self * k::Isometry3::from_parts(
            self.translation.swap_yz_axis(),
            self.rotation.swap_yz_axis_and_flip_hand(),
            // k::nalgebra::UnitQuaternion::from_matrix(&k::nalgebra::Matrix3::new(
            //     1.0, 0.0, 0.0, // X-axis remains the same
            //     0.0, 0.0, 1.0, // Y-axis becomes Z
            //     0.0, -1.0, 0.0, // Z-axis becomes -Y
            // )),
        )
    }
}
