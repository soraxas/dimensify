// use crate::prelude::{Quat, Vec2, Vec3, Vec4};
// use serde::{Deserialize, Serialize};
// use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

// #[repr(C)]
// #[derive(
//     Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
// )]
// pub struct WktHeader {
//     pub kind: u32,
//     pub version: u16,
//     pub flags: u16,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(tag = "type")]
// pub enum Component {
//     Name {
//         value: String,
//     },
//     Transform {
//         translation: Vec3,
//         rotation: Quat,
//         scale: Vec3,
//     },
//     // Line3d {
//     //     #[serde(default)]
//     //     name: Option<String>,
//     //     points: Vec<ProtoVec3>,
//     //     color: ProtoVec4,
//     //     width: f32,
//     // },
//     // Line2d {
//     //     #[serde(default)]
//     //     name: Option<String>,
//     //     points: Vec<ProtoVec2>,
//     //     color: ProtoVec4,
//     //     width: f32,
//     // },
//     // Text3d {
//     //     #[serde(default)]
//     //     name: Option<String>,
//     //     text: String,
//     //     position: ProtoVec3,
//     //     color: ProtoVec4,
//     // },
//     // Text2d {
//     //     #[serde(default)]
//     //     name: Option<String>,
//     //     text: String,
//     //     position: ProtoVec2,
//     //     color: ProtoVec4,
//     // },
//     // Mesh3d {
//     //     #[serde(default)]
//     //     name: Option<String>,
//     //     position: ProtoVec3,
//     //     scale: ProtoVec3,
//     // },
//     // Rect2d {
//     //     #[serde(default)]
//     //     name: Option<String>,
//     //     position: ProtoVec2,
//     //     size: ProtoVec2,
//     //     rotation: f32,
//     //     color: ProtoVec4,
//     // },
//     // Transform3d {
//     //     transform: ProtoTransform,
//     // },
//     // Binary {
//     //     header: WktHeader,
//     //     payload: Vec<u8>,
//     //     meta: Option<String>,
//     // },
// }

// use bevy::prelude::{
//     Mesh2d as BevyMesh2d, Mesh3d as BevyMesh3d, Name as BevyName, Transform as BevyTransform,
// };
// // use bevy::prelude::Material3d as BevyMaterial3d;
// // use bevy::prelude::Material2d as BevyMaterial2d;
// // use bevy::prelude::Material3d as BevyMaterial3d;

// // struct  Mesh {
// //     primitive_topology: PrimitiveTopology,
// //     /// `std::collections::BTreeMap` with all defined vertex attributes (Positions, Normals, ...)
// //     /// for this mesh. Attribute ids to attribute values.
// //     /// Uses a [`BTreeMap`] because, unlike `HashMap`, it has a defined iteration order,
// //     /// which allows easy stable `VertexBuffers` (i.e. same buffer order)
// //     attributes: BTreeMap<MeshVertexAttributeId, MeshAttributeData>,
// //     indices: Option<Indices>,
// //     morph_targets: Option<Handle<Image>>,
// //     morph_target_names: Option<Vec<String>>,
// //     asset_usage: RenderAssetUsages,
// //     enable_raytracing: bool,
// // }

// // #[cfg(feature = "bevy")]
// // mod bevy_impls {
// //     use super::*;
// //     use bevy::prelude::{
// //         Mesh2d as BevyMesh2d, Mesh3d as BevyMesh3d, Quat as BevyQuat, Transform as BevyTransform,
// //         Vec2 as BevyVec2, Vec3 as BevyVec3,
// //     };

// //     macro_rules! impl_into_bevy {
// //         ($from:ident => $bevy:ty, |$arg:ident| $body:expr) => {
// //             impl Into<$bevy> for $from {
// //                 fn into(self) -> $bevy {
// //                     let $arg = self;
// //                     $body
// //                 }
// //             }
// //         };
// //     }

// //     impl_into_bevy!(ProtoVec2 => BevyVec2, |v| BevyVec2::new(v.0[0], v.0[1]));
// //     impl_into_bevy!(ProtoVec3 => BevyVec3, |v| BevyVec3::new(v.0[0], v.0[1], v.0[2]));
// //     impl_into_bevy!(ProtoQuat => BevyQuat, |q| BevyQuat::from_xyzw(q.0[0], q.0[1], q.0[2], q.0[3]));
// //     impl_into_bevy!(ProtoTransform => BevyTransform, |t| {
// //         BevyTransform {
// //             translation: t.position.into(),
// //             rotation: t.rotation.into(),
// //             scale: t.scale.into(),
// //         }
// //     });
// // }
