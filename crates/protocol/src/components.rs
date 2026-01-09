use serde::{Deserialize, Serialize};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use bevy_ecs::component::Component;

pub trait DimensifyComponent {}

macro_rules! component_struct {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout)]
        pub struct $name {
            $( $field: $ty ),*
        }
    };
}

component_struct!(Vec2 { x: f32, y: f32 });
component_struct!(Vec3 {
    x: f32,
    y: f32,
    z: f32
});
component_struct!(Quat {
    x: f32,
    y: f32,
    z: f32,
    w: f32
});
component_struct!(Transform {
    position: Vec3,
    rotation: Quat,
    scale: Vec3,
});

use std::collections::BTreeMap;

// struct  Mesh {
//     primitive_topology: PrimitiveTopology,
//     /// `std::collections::BTreeMap` with all defined vertex attributes (Positions, Normals, ...)
//     /// for this mesh. Attribute ids to attribute values.
//     /// Uses a [`BTreeMap`] because, unlike `HashMap`, it has a defined iteration order,
//     /// which allows easy stable `VertexBuffers` (i.e. same buffer order)
//     attributes: BTreeMap<MeshVertexAttributeId, MeshAttributeData>,
//     indices: Option<Indices>,
//     morph_targets: Option<Handle<Image>>,
//     morph_target_names: Option<Vec<String>>,
//     asset_usage: RenderAssetUsages,
//     enable_raytracing: bool,
// }

#[cfg(feature = "bevy")]
mod bevy_impls {
    use super::*;
    use bevy::prelude::{
        Mesh2d as BevyMesh2d, Mesh3d as BevyMesh3d, Quat as BevyQuat, Transform as BevyTransform,
        Vec2 as BevyVec2, Vec3 as BevyVec3,
    };

    macro_rules! impl_into_bevy {
        ($from:ident => $bevy:ty, |$arg:ident| $body:expr) => {
            impl Into<$bevy> for $from {
                fn into(self) -> $bevy {
                    let $arg = self;
                    $body
                }
            }
        };
    }

    impl_into_bevy!(Vec2 => BevyVec2, |v| BevyVec2::new(v.x, v.y));
    impl_into_bevy!(Vec3 => BevyVec3, |v| BevyVec3::new(v.x, v.y, v.z));
    impl_into_bevy!(Quat => BevyQuat, |q| BevyQuat::from_xyzw(q.x, q.y, q.z, q.w));
    impl_into_bevy!(Transform => BevyTransform, |t| {
        BevyTransform {
            translation: t.position.into(),
            rotation: t.rotation.into(),
            scale: t.scale.into(),
        }
    });
}

pub mod prelude {
    #[cfg(feature = "bevy")]
    pub use super::bevy_impls::*;
    pub use super::{Quat, Transform, Vec2, Vec3};
}
