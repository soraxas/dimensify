#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use super::helpers::bevy_mesh;
use bevy::asset::{Assets, Handle};
use bevy::math::Vec3;
use bevy::pbr::StandardMaterial;
use bevy::prelude::{Mesh, Resource};
use rapier3d::geometry::{Cone, Cylinder, ShapeType};
use rapier3d::na::Point3;
use rapier3d::prelude::Shape;
use std::collections::HashMap;

const HALFSPACE_HALF_SIDE: f32 = 1000.0;

#[derive(Debug, Resource, Default)]
pub struct PrefabAssets {
    meshes_strong_handles: HashMap<ShapeType, Handle<Mesh>>,
    pub default_material: Handle<StandardMaterial>,
}

impl PrefabAssets {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut meshes_strong_handles = HashMap::new();
        Self::gen_prefab_meshes(&mut meshes_strong_handles, meshes);
        Self {
            meshes_strong_handles,
            default_material: materials.add(StandardMaterial {
                ..Default::default()
            }),
        }
    }

    pub fn clear(&mut self) {
        self.meshes_strong_handles.clear();
    }

    // pub fn initialise_if_empty(&mut self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
    //     if self.meshes_strong_handles.is_empty() {
    //         Self::gen_prefab_meshes(&mut self.meshes_strong_handles, meshes, materials);
    //     }
    // }

    pub fn is_supported(&self, shape_type: ShapeType) -> bool {
        matches!(
            shape_type,
            ShapeType::Ball
                | ShapeType::Cuboid
                | ShapeType::RoundCuboid
                | ShapeType::Cylinder
                | ShapeType::RoundCylinder
                | ShapeType::Cone
                | ShapeType::RoundCone
                | ShapeType::HalfSpace
        )
    }

    pub fn get_prefab_mesh_handle(&self, shape_type: &ShapeType) -> &Handle<Mesh> {
        &self.meshes_strong_handles[shape_type]
    }

    pub fn maybe_get_strong_prefab_mesh_handle(
        &self,
        shape_type: &ShapeType,
    ) -> Option<Handle<Mesh>> {
        if self.meshes_strong_handles.is_empty() {
            panic!("PrefabAssets is empty");
        }
        self.meshes_strong_handles.get(shape_type).cloned()
    }

    /// This will return the correct scale for the prefab mesh of the given shape type.
    pub fn get_mesh_scale(co_shape: &dyn Shape) -> Option<Vec3> {
        match co_shape.shape_type() {
            ShapeType::Ball => {
                let b = co_shape.as_ball().unwrap();
                Some(Vec3::new(b.radius as f32, b.radius as f32, b.radius as f32))
            }
            ShapeType::Cuboid => {
                let c = co_shape.as_cuboid().unwrap();
                Some(Vec3::from_slice(c.half_extents.cast::<f32>().as_slice()))
            }
            ShapeType::RoundCuboid => {
                let c = co_shape.as_round_cuboid().unwrap();
                Some(Vec3::from_slice(
                    c.inner_shape.half_extents.cast::<f32>().as_slice(),
                ))
            }
            ShapeType::Cylinder => {
                let c = co_shape.as_cylinder().unwrap();
                Some(Vec3::new(
                    c.radius as f32,
                    c.half_height as f32,
                    c.radius as f32,
                ))
            }
            ShapeType::RoundCylinder => {
                let c = &co_shape.as_round_cylinder().unwrap().inner_shape;
                Some(Vec3::new(
                    c.radius as f32,
                    c.half_height as f32,
                    c.radius as f32,
                ))
            }
            ShapeType::Cone => {
                let c = co_shape.as_cone().unwrap();
                Some(Vec3::new(
                    c.radius as f32,
                    c.half_height as f32,
                    c.radius as f32,
                ))
            }
            ShapeType::RoundCone => {
                let c = &co_shape.as_round_cone().unwrap().inner_shape;
                Some(Vec3::new(
                    c.radius as f32,
                    c.half_height as f32,
                    c.radius as f32,
                ))
            }
            ShapeType::HalfSpace => Some(Vec3::ONE),
            _ => None,
            // _ => panic!(
            //     "This shape type is not supported by the prefab meshes: {:?}",
            //     co_shape.shape_type()
            // ),
        }
    }

    // /// This will return the correct scale for the prefab mesh of the given shape type.
    // pub fn get_mesh_scale_from_urdf(shape: &Geometry) -> Option<Vec3> {
    //     match shape {
    //         Geometry::Box { size } => {
    //             // return the half extents
    //             Some(Vec3::new(
    //                 (size[0] as f32) / 2.,
    //                 (size[1] as f32) / 2.,
    //                 (size[2] as f32) / 2.,
    //             ))
    //         }
    //         Geometry::Cylinder { radius, length } => {
    //             Some(Vec3::new(
    //                 (*radius as f32),
    //                 (*length as f32) / 2.,  // half height
    //                 (*radius as f32),
    //             ))
    //         },
    //         Geometry::Capsule { radius, length } => {
    //             Some(Vec3::new(
    //                 (*radius as f32),
    //                 (*length as f32) / 2.,  // half height
    //                 (*radius as f32),
    //             ))
    //         },
    //         Geometry::Sphere { radius } => todo!(),
    //         Geometry::Mesh { filename, scale } => todo!(),
    //     }

    //     match co_shape.shape_type() {
    //         ShapeType::Ball => {
    //             let b = co_shape.as_ball().unwrap();
    //             Some(Vec3::new(b.radius as f32, b.radius as f32, b.radius as f32))
    //         }
    //         ShapeType::Cuboid => {
    //             let c = co_shape.as_cuboid().unwrap();
    //             Some(Vec3::from_slice(c.half_extents.cast::<f32>().as_slice()))
    //         }
    //         ShapeType::RoundCuboid => {
    //             let c = co_shape.as_round_cuboid().unwrap();
    //             Some(Vec3::from_slice(
    //                 c.inner_shape.half_extents.cast::<f32>().as_slice(),
    //             ))
    //         }
    //         ShapeType::Cylinder => {
    //             let c = co_shape.as_cylinder().unwrap();
    //             Some(Vec3::new(
    //                 c.radius as f32,
    //                 c.half_height as f32,
    //                 c.radius as f32,
    //             ))
    //         }
    //         ShapeType::RoundCylinder => {
    //             let c = &co_shape.as_round_cylinder().unwrap().inner_shape;
    //             Some(Vec3::new(
    //                 c.radius as f32,
    //                 c.half_height as f32,
    //                 c.radius as f32,
    //             ))
    //         }
    //         ShapeType::Cone => {
    //             let c = co_shape.as_cone().unwrap();
    //             Some(Vec3::new(
    //                 c.radius as f32,
    //                 c.half_height as f32,
    //                 c.radius as f32,
    //             ))
    //         }
    //         ShapeType::RoundCone => {
    //             let c = &co_shape.as_round_cone().unwrap().inner_shape;
    //             Some(Vec3::new(
    //                 c.radius as f32,
    //                 c.half_height as f32,
    //                 c.radius as f32,
    //             ))
    //         }
    //         ShapeType::HalfSpace => Some(Vec3::ONE),
    //         _ => None,
    //         // _ => panic!(
    //         //     "This shape type is not supported by the prefab meshes: {:?}",
    //         //     co_shape.shape_type()
    //         // ),
    //     }
    // }

    fn gen_prefab_meshes(
        container: &mut HashMap<ShapeType, Handle<Mesh>>,
        meshes: &mut Assets<Mesh>,
    ) {
        //
        // Cuboid mesh
        //
        let cuboid = Mesh::from(bevy::math::primitives::Cuboid::new(2.0, 2.0, 2.0));
        container.insert(ShapeType::Cuboid, meshes.add(cuboid.clone()));
        container.insert(ShapeType::RoundCuboid, meshes.add(cuboid));

        //
        // Ball mesh
        //
        let ball = Mesh::from(bevy::math::primitives::Sphere::new(1.0));
        container.insert(ShapeType::Ball, meshes.add(ball));

        //
        // Cylinder mesh
        //
        let cylinder = Cylinder::new(1.0, 1.0);
        let mesh = bevy_mesh(cylinder.to_trimesh(20));
        container.insert(ShapeType::Cylinder, meshes.add(mesh.clone()));
        container.insert(ShapeType::RoundCylinder, meshes.add(mesh));

        //
        // Cone mesh
        //
        let cone = Cone::new(1.0, 1.0);
        let mesh = bevy_mesh(cone.to_trimesh(10));
        container.insert(ShapeType::Cone, meshes.add(mesh.clone()));
        container.insert(ShapeType::RoundCone, meshes.add(mesh));

        //
        // Halfspace
        //
        let vertices = vec![
            Point3::new(-HALFSPACE_HALF_SIDE, 0.0, -HALFSPACE_HALF_SIDE),
            Point3::new(HALFSPACE_HALF_SIDE, 0.0, -HALFSPACE_HALF_SIDE),
            Point3::new(HALFSPACE_HALF_SIDE, 0.0, HALFSPACE_HALF_SIDE),
            Point3::new(-HALFSPACE_HALF_SIDE, 0.0, HALFSPACE_HALF_SIDE),
        ];
        let indices = vec![[0, 1, 2], [0, 2, 3]];
        let mesh = bevy_mesh((vertices, indices));
        container.insert(ShapeType::HalfSpace, meshes.add(mesh));
    }
}
