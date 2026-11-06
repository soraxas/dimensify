use bevy::math::Vec3 as BevyVec3;

#[cfg(feature = "robot")]
use urdf_rs::Vec3 as UrdfVec3;

// use bevy::math::Quat as BevyQuat;
// use bevy::prelude::Transform as BevyTransform;

// use super::prelude::SwapYZandFlipHandTrait;

// pub trait FromBevySwapYZandFlipHandTrait {
//     fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyTransform) -> Self;
// }

// pub trait FromBevySwapYZandFlipHandRotationTrait {
//     fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyQuat) -> Self;
// }

pub trait ToBevyVecSwapYZTrait {
    fn to_bevy_with_swap_yz_axis(&self) -> BevyVec3;
}

////////////////////////////////////////////////////////////

// pub trait CoordSysTranslationFromBevy {
//     fn from_bevy_with_swap_yz(val: BevyVec3) -> Self;
// }

// ////////////////////////////////////////////////
// // conversion between bevy translation and nalgebra translation

// impl CoordSysTranslationFromBevy for k::Translation3<f32> {
//     #[inline(always)]
//     #[must_use]
//     fn from_bevy_with_swap_yz(val: BevyVec3) -> Self {
//         k::Translation3::new(val.x, val.z, val.y)
//     }
// }

// impl FromBevySwapYZandFlipHandRotationTrait for k::nalgebra::UnitQuaternion<f32> {
//     #[inline(always)]
//     #[must_use]
//     fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyQuat) -> Self {
//         let val = k::nalgebra::UnitQuaternion::from_quaternion(k::nalgebra::Quaternion::new(
//             val.w, val.x, val.y, val.z,
//         ));
//         val.swap_yz_axis_and_flip_hand()
//     }
// }

// impl FromBevySwapYZandFlipHandTrait for k::nalgebra::Isometry3<f32> {
//     #[inline(always)]
//     #[must_use]
//     fn from_bevy_with_swap_yz_axis_and_flip_hand(val: &BevyTransform) -> Self {
//         k::Isometry3::from_parts(
//             k::nalgebra::Translation3::from_bevy_with_swap_yz(val.translation),
//             k::nalgebra::UnitQuaternion::from_bevy_with_swap_yz_axis_and_flip_hand(&val.rotation),
//         )
//     }
// }

#[cfg(feature = "robot")]
impl ToBevyVecSwapYZTrait for UrdfVec3 {
    #[inline(always)]
    fn to_bevy_with_swap_yz_axis(&self) -> BevyVec3 {
        BevyVec3::new(self[0] as f32, self[2] as f32, self[1] as f32)
    }
}
