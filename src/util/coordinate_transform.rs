use std::f32::consts::FRAC_PI_2;

use bevy::math::Quat as BevyQuat;
use bevy::math::Vec3 as BevyVec3;
use bevy::prelude::Transform as BevyTransform;
use urdf_rs::Vec3 as UrdfVec3;

// coordinate system conversion: https://bevy-cheatbook.github.io/fundamentals/coords.html

pub trait CoordSysTranslationToBevy {
    fn to_bevy(&self) -> BevyVec3;
}

pub trait CoordSysTranslationFromBevy {
    fn from_bevy(val: BevyVec3) -> Self;
}

////////////////////////////////////////////////
// conversion between bevy translation and nalgebra translation

impl CoordSysTranslationToBevy for k::Translation3<f32> {
    #[inline(always)]
    #[must_use]
    fn to_bevy(&self) -> BevyVec3 {
        BevyVec3::new(
            self.x, self.z, // swap z with bevy's y
            self.y,
        )
    }
}

impl CoordSysTranslationFromBevy for k::Translation3<f32> {
    #[inline(always)]
    #[must_use]
    fn from_bevy(val: BevyVec3) -> Self {
        k::Translation3::new(
            val.x, val.z, // swap z with bevy's y
            val.y,
        )
    }
}

#[test]
fn test_translation_conversion() {
    let bevy_translation = BevyVec3::new(1.0, 2.0, 3.0);
    let nalgebra_translation = k::Translation3::new(1.0, 3.0, 2.0);
    assert_eq!(
        nalgebra_translation,
        k::Translation3::from_bevy(bevy_translation)
    );
    assert_eq!(bevy_translation, nalgebra_translation.to_bevy());
}

////////////////////////////////////////////////
// conversion between bevy rotation and nalgebra rotation

pub trait CoordSysRotationToBevy {
    fn to_bevy(&self) -> BevyQuat;
}

pub trait CoordSysRotationFromBevy {
    fn from_bevy(val: &BevyQuat) -> Self;
}

////////////////////////////////////////////////

impl CoordSysRotationToBevy for k::nalgebra::UnitQuaternion<f32> {
    #[inline(always)]
    #[must_use]
    fn to_bevy(&self) -> BevyQuat {
        //TODO: check if this is correct
        BevyQuat::from_xyzw(
            self.i, -self.k, // swap z with bevy's y  // negative y axis rotational
            self.j, self.w,
        )
    }
}

impl CoordSysRotationFromBevy for k::nalgebra::UnitQuaternion<f32> {
    #[inline(always)]
    #[must_use]
    fn from_bevy(val: &BevyQuat) -> Self {
        k::nalgebra::UnitQuaternion::from_quaternion(k::nalgebra::Quaternion::new(
            // val.rotation.w,
            // val.rotation.x,
            // -val.rotation.z,
            // val.rotation.y,
            val.w, val.x, -val.z, // swap bevy's y with z  // negative y axis rotational
            val.y,
        ))
    }
}

////////////////////////////////////////////////
// conversion between bevy transform and nalgebra isometry

pub trait CoordSysTransformToBevy {
    fn to_bevy(&self) -> BevyTransform;
}

pub trait CoordSysTransformFromBevy {
    fn from_bevy(val: &BevyTransform) -> Self;
}

////////////////////////////////////////////////

impl CoordSysTransformFromBevy for k::Isometry3<f32> {
    fn from_bevy(val: &BevyTransform) -> Self {
        // trans.rotate_local_x(-FRAC_PI_2);
        k::Isometry3::from_parts(
            k::nalgebra::Translation3::from_bevy(val.translation),
            k::nalgebra::UnitQuaternion::from_bevy(&val.rotation),
        )
    }
}

pub trait CoordinateSysToBevy {
    fn to_bevy(self) -> BevyVec3;
}

impl CoordinateSysToBevy for UrdfVec3 {
    #[inline(always)]
    #[must_use]
    fn to_bevy(self) -> BevyVec3 {
        let translation = self;
        // swap urdf's z with bevy's y
        BevyVec3 {
            x: translation[0] as f32,
            y: translation[2] as f32,
            z: translation[1] as f32,
        }
    }
}

pub trait CoordinateSysTransformToBevy {
    fn to_bevy_inplace(&mut self);
    fn to_bevy(self) -> Self;
}

impl CoordinateSysTransformToBevy for BevyTransform {
    #[inline(always)]
    fn to_bevy_inplace(&mut self) {
        // urdf uses z-axis as the up axis, while bevy uses y-axis as the up axis
        self.rotate_local_x(-FRAC_PI_2)
    }

    // a consuming version for the above function
    #[inline(always)]
    #[must_use]
    fn to_bevy(mut self) -> Self {
        self.to_bevy_inplace();
        self
    }
}
