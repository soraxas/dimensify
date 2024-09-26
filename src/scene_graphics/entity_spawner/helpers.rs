use crate::scene_graphics::helpers::bevy_mesh;
use bevy::asset::{Assets, Handle};
use bevy::prelude::Mesh;
use rapier3d::geometry::{Cone, Cylinder, ShapeType};
use rapier3d::prelude::point;
use std::collections::HashMap;

pub fn gen_prefab_meshes(out: &mut HashMap<ShapeType, Handle<Mesh>>, meshes: &mut Assets<Mesh>) {
    //
    // Cuboid mesh
    //
    let cuboid = Mesh::from(bevy::math::primitives::Cuboid::new(2.0, 2.0, 2.0));
    out.insert(ShapeType::Cuboid, meshes.add(cuboid.clone()));
    out.insert(ShapeType::RoundCuboid, meshes.add(cuboid));

    //
    // Ball mesh
    //
    let ball = Mesh::from(bevy::math::primitives::Sphere::new(1.0));
    out.insert(ShapeType::Ball, meshes.add(ball));

    //
    // Cylinder mesh
    //
    let cylinder = Cylinder::new(1.0, 1.0);
    let mesh = bevy_mesh(cylinder.to_trimesh(20));
    out.insert(ShapeType::Cylinder, meshes.add(mesh.clone()));
    out.insert(ShapeType::RoundCylinder, meshes.add(mesh));

    //
    // Cone mesh
    //
    let cone = Cone::new(1.0, 1.0);
    let mesh = bevy_mesh(cone.to_trimesh(10));
    out.insert(ShapeType::Cone, meshes.add(mesh.clone()));
    out.insert(ShapeType::RoundCone, meshes.add(mesh));

    //
    // Halfspace
    //
    let vertices = vec![
        point![-1000.0, 0.0, -1000.0],
        point![1000.0, 0.0, -1000.0],
        point![1000.0, 0.0, 1000.0],
        point![-1000.0, 0.0, 1000.0],
    ];
    let indices = vec![[0, 1, 2], [0, 2, 3]];
    let mesh = bevy_mesh((vertices, indices));
    out.insert(ShapeType::HalfSpace, meshes.add(mesh));
}
