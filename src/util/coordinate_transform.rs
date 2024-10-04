use std::f32::consts::FRAC_PI_2;

use bevy::math::Vec3 as BevyVec3;
use bevy::prelude::Transform as BevyTransform;
use urdf_rs::Vec3 as UrdfVec3;

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
