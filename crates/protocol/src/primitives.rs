use serde::{Deserialize, Serialize};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};
// #[cfg(feature = "zerocopy")]

use bevy_math::Vec3;

/// Define a POD vector type with serialization/deserialization support.
macro_rules! pod_vec {
    ($name:ident, $len:expr) => {
        #[repr(C)]
        #[derive(
            Debug, Clone, Copy, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
        )]
        #[serde(transparent)]
        pub struct $name(pub [f32; $len]);

        impl From<[f32; $len]> for $name {
            fn from(value: [f32; $len]) -> Self {
                Self(value)
            }
        }

        impl From<$name> for [f32; $len] {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[cfg(feature = "bevy")]
pub mod bevy_impls {
    use super::*;
    use bevy::prelude::{Quat as BevyQuat, Vec3 as BevyVec3};
}
