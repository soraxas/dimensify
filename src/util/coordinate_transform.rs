use std::f32::consts::FRAC_PI_2;

use bevy::math::Mat4;
use bevy::math::Quat as BevyQuat;
use bevy::math::Vec3 as BevyVec3;
use bevy::math::Vec4 as BevyVec4;
use bevy::prelude::Transform as BevyTransform;
use urdf_rs::Vec3 as UrdfVec3;

pub trait FromBevySwapYZandFlipHandTrait {
    fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyTransform) -> Self;
}

pub trait FromBevySwapYZandFlipHandRotationTrait {
    fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyQuat) -> Self;
}

pub trait SwapYZandFlipHandTrait {
    fn swap_yz_axis_and_flip_hand(&self) -> Self;
}

pub trait SwapYZTrait {
    fn swap_yz_axis(&self) -> Self;
}

pub trait ToBevySwapYZTrait {
    fn to_bevy_with_swap_yz_axis(&self) -> BevyVec3;
}

impl SwapYZTrait for BevyVec3 {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis(&self) -> Self {
        BevyVec3::new(self.x, self.z, self.y)
    }
}

impl ToBevySwapYZTrait for UrdfVec3 {
    #[inline(always)]
    #[must_use]
    fn to_bevy_with_swap_yz_axis(&self) -> BevyVec3 {
        BevyVec3::new(self[0] as f32, self[2] as f32, self[1] as f32)
    }
}

impl SwapYZTrait for k::nalgebra::Translation3<f32> {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis(&self) -> Self {
        k::nalgebra::Translation3::new(self.x, self.z, self.y)
    }
}

impl SwapYZandFlipHandTrait for k::nalgebra::UnitQuaternion<f32> {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self {
        self * k::nalgebra::UnitQuaternion::from_matrix(&k::nalgebra::Matrix3::new(
            1.0, 0.0, 0.0, // X-axis remains the same
            0.0, 0.0, 1.0, // Y-axis becomes -Z
            0.0, -1.0, 0.0, // Z-axis becomes Y
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
            //     0.0, 0.0, 1.0, // Y-axis becomes -Z
            //     0.0, -1.0, 0.0, // Z-axis becomes Y
            // )),
        )
    }
}

// coordinate system conversion: https://bevy-cheatbook.github.io/fundamentals/coords.html

pub trait CoordSysTranslationFromBevy {
    fn from_bevy_with_swap_yz(val: BevyVec3) -> Self;
}

////////////////////////////////////////////////
// conversion between bevy translation and nalgebra translation

impl CoordSysTranslationFromBevy for k::Translation3<f32> {
    #[inline(always)]
    #[must_use]
    fn from_bevy_with_swap_yz(val: BevyVec3) -> Self {
        k::Translation3::new(val.x, val.z, val.y)
    }
}

////////////////////////////////////////////////
// conversion between bevy rotation and nalgebra rotation

////////////////////////////////////////////////

impl FromBevySwapYZandFlipHandRotationTrait for k::nalgebra::UnitQuaternion<f32> {
    #[inline(always)]
    #[must_use]
    fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyQuat) -> Self {
        let val = k::nalgebra::UnitQuaternion::from_quaternion(k::nalgebra::Quaternion::new(
            val.w, val.x, val.y, val.z,
        ));
        val.swap_yz_axis_and_flip_hand()
    }
}

impl FromBevySwapYZandFlipHandTrait for k::nalgebra::Isometry3<f32> {
    #[inline(always)]
    #[must_use]
    fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyTransform) -> Self {
        k::Isometry3::from_parts(
            k::nalgebra::Translation3::from_bevy_with_swap_yz(val.translation),
            k::nalgebra::UnitQuaternion::from_bevy_with_swap_yz_axis_and_flip_hand(&val.rotation),
        )
    }
}

////////////////////////////////////////////////

impl SwapYZandFlipHandTrait for BevyTransform {
    #[inline(always)]
    #[must_use]
    fn swap_yz_axis_and_flip_hand(&self) -> Self {
        self.mul_transform(BevyTransform::from_matrix(Mat4 {
            x_axis: BevyVec4::new(1.0, 0.0, 0.0, 0.0), // X-axis remains the same
            y_axis: BevyVec4::new(0.0, 0.0, 1.0, 0.0), // Y-axis becomes -Z
            z_axis: BevyVec4::new(0.0, -1.0, 0.0, 0.0), // Z-axis becomes Y
            w_axis: BevyVec4::new(0.0, 0.0, 0.0, 1.0), // Homogeneous component
        }))
    }
}
