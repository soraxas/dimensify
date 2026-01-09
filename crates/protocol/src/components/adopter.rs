use crate::prelude::{Quat, Vec2, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::bm3d;
use bevy::prelude::Component as BevyComponent;

// TODO: add supported mode (2d, 3d, both)
// #[cfg(feature = "bevy")]
// fn supports<'a>(
//     self,
//     e: &mut bevy::prelude::EntityCommands<'a>,
// ) -> bevy::prelude::EntityCommands<'a> {
//     self.insert_into(e)
// }

// pub enum Shape3d {
//     Cuboid {
//         half_size: ProtoVec3,
//     },
//     Cylinder {
//         radius: f32,
//         half_height: f32,
//     },
// }

use bevy::pbr::StandardMaterial;

#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Material {
    Color { r: f32, g: f32, b: f32, a: f32 },
}

impl Material {
    pub fn material(self) -> StandardMaterial {
        use bevy::prelude::Color;
        match self {
            Material::Color { r, g, b, a } => Color::srgba(r, g, b, a).into(),
        }
    }
}

/// Shape primitives that map to Bevy meshes.
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Shape3d {
    Sphere(bm3d::Sphere),
    Plane3d(bm3d::Plane3d),
    // InfinitePlane3d(bm3d::InfinitePlane3d),
    // Line3d(bm3d::Line3d),
    Segment3d(bm3d::Segment3d),
    Polyline3d(bm3d::Polyline3d),
    Cuboid(bm3d::Cuboid),
    Cylinder(bm3d::Cylinder),
    Capsule3d(bm3d::Capsule3d),
    Cone(bm3d::Cone),
    ConicalFrustum(bm3d::ConicalFrustum),
    Torus(bm3d::Torus),
    Triangle3d(bm3d::Triangle3d),
    Tetrahedron(bm3d::Tetrahedron),
}

impl Shape3d {
    pub fn mesh(self) -> bevy::mesh::Mesh {
        match self {
            Shape3d::Sphere(sphere) => sphere.into(),
            Shape3d::Plane3d(plane) => plane.into(),
            // Shape3d::InfinitePlane3d(infinite_plane) => infinite_plane.into(),
            // Shape3d::Line3d(line) => line.into(),
            Shape3d::Segment3d(segment) => segment.into(),
            Shape3d::Polyline3d(polyline) => polyline.into(),
            Shape3d::Cuboid(cuboid) => cuboid.into(),
            Shape3d::Cylinder(cylinder) => cylinder.into(),
            Shape3d::Capsule3d(capsule) => capsule.into(),
            Shape3d::Cone(cone) => cone.into(),
            Shape3d::ConicalFrustum(conical_frustum) => conical_frustum.into(),
            Shape3d::Torus(torus) => torus.into(),
            Shape3d::Triangle3d(triangle) => triangle.into(),
            Shape3d::Tetrahedron(tetrahedron) => tetrahedron.into(),
        }
    }
}

/// Protocol component payloads used by `WorldCommand`.
#[repr(C)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtoComponent {
    Name(String),
    Transform {
        translation: Vec3,
        rotation: Quat,
        scale: Vec3,
    },
    Mesh3d(Shape3d),
    MeshMaterial3d(Material),
}

use bevy::{asset::Assets, mesh::Mesh};

#[cfg(feature = "bevy")]
pub mod bevy_impls {
    use super::*;
    use crate::components::{bevy_impls::InsertionResult, prelude::ProtoComponentIntoBevy};
    use bevy::prelude::{Name as BevyName, Transform as BevyTransform};

    impl ProtoComponentIntoBevy for ProtoComponent {
        #[cfg(feature = "bevy")]
        fn insert_into<'a, 'b>(
            self,
            e: &'a mut bevy::prelude::EntityCommands<'b>,
        ) -> InsertionResult<'a, 'b> {
            use bevy::log::info;
            info!("Inserting component: {:?} into entity: {:?}", self, e.id());

            match self {
                ProtoComponent::Name(name) => {
                    InsertionResult::Trivial(e.insert(BevyName::new(name)))
                }
                ProtoComponent::Transform {
                    translation,
                    rotation,
                    scale,
                } => InsertionResult::Trivial(e.insert(BevyTransform {
                    translation: translation.into(),
                    rotation: rotation.into(),
                    scale: scale.into(),
                })),
                ProtoComponent::Mesh3d(shape) => InsertionResult::RequireResMesh(shape),
                ProtoComponent::MeshMaterial3d(material) => {
                    InsertionResult::RequireResMaterial(material)
                } // ProtoComponent::Mesh3d(shape) => {

                  //     let handle = meshes.add(shape.mesh());

                  //     e.insert(BevyMesh3d(handle))
                  // }
                  // ProtoComponent::MeshMaterial3d(material) => {
                  //     let handle = materials.add(material.material());
                  //     e.insert(BevyMeshMaterial3d(handle))
                  // }
            }
        }
    }
}
