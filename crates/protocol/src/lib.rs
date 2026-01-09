mod components;
mod errors;
mod requests;
mod telemetry;

pub use errors::TransportError;
pub use requests::*;
pub use telemetry::*;

pub mod bm3d {
    pub use bevy_math::primitives::{
        Capsule3d,
        Cone,
        ConicalFrustum,
        Cuboid,
        Cylinder,
        Plane3d,
        Polyline3d,
        Segment3d,
        // 3d primitives
        Sphere,
        Tetrahedron,
        Torus,
        Triangle3d,
    };
}

#[cfg(feature = "bevy")]
pub mod bevy_impls {
    pub use super::components::bevy_impls::*;
}

pub mod prelude {
    pub use super::{
        components::prelude::{Material, ProtoComponent, Shape3d},
        requests::{EntityInfo, ProtoResponse, WorldCommand},
    };
    pub use bevy_ecs::entity::Entity;
    pub use bevy_math::{Dir2, Dir3, Dir4, InvalidDirectionError, Quat, Vec2, Vec3, Vec4};

    #[cfg(feature = "bevy")]
    pub use super::bevy_impls::*;

    // renamed
    pub use super::components::prelude::ProtoComponent as Component;
    // pub use super::primitives::*;
    // pub use super::requests::*;
    // pub use super::telemetry::*;
}
