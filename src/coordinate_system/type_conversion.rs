use bevy::math::Quat as BevyQuat;
use bevy::math::Vec3 as BevyVec3;
use bevy::prelude::Transform as BevyTransform;

pub trait FromBevyTransform {
    #[must_use]
    fn from_bevy(val: &BevyTransform) -> Self;
}

pub trait ToBevyTransform {
    #[must_use]
    fn to_bevy(self) -> BevyTransform;
}

////////////////////////////////////////////////

impl FromBevyTransform for k::nalgebra::Isometry3<f32> {
    #[inline(always)]
    #[must_use]
    fn from_bevy(val: &BevyTransform) -> Self {
        k::Isometry3::from_parts(
            k::Translation3::new(val.translation.x, val.translation.y, val.translation.z),
            k::UnitQuaternion::from_quaternion(k::nalgebra::Quaternion::new(
                val.rotation.w,
                val.rotation.x,
                val.rotation.y,
                val.rotation.z,
            )),
        )
    }
}

impl ToBevyTransform for k::nalgebra::Isometry3<f32> {
    #[inline(always)]
    #[must_use]
    fn to_bevy(self) -> BevyTransform {
        BevyTransform {
            translation: BevyVec3::new(self.translation.x, self.translation.y, self.translation.z),
            rotation: BevyQuat::from_xyzw(
                self.rotation.i,
                self.rotation.j,
                self.rotation.k,
                self.rotation.w,
            ),
            scale: BevyVec3::ONE,
        }
    }
}
