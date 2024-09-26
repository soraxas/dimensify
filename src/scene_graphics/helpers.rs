#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::prelude::*;

use bevy::render::mesh::{Indices, VertexAttributeValues};
use bevy::render::render_resource::PrimitiveTopology;
use na::{Point3, Vector3};
use rapier3d::geometry::{ColliderHandle, ColliderSet, Shape, ShapeType};
use rapier3d::math::{Isometry, Real, Vector};

pub(crate) fn bevy_mesh(buffers: (Vec<Point3<Real>>, Vec<[u32; 3]>)) -> Mesh {
    let (vtx, idx) = buffers;
    let mut normals: Vec<[f32; 3]> = vec![];
    let mut vertices: Vec<[f32; 3]> = vec![];

    for idx in idx {
        let a = vtx[idx[0] as usize];
        let b = vtx[idx[1] as usize];
        let c = vtx[idx[2] as usize];

        vertices.push(a.cast::<f32>().into());
        vertices.push(b.cast::<f32>().into());
        vertices.push(c.cast::<f32>().into());
    }

    for vtx in vertices.chunks(3) {
        let a = Point3::from(vtx[0]);
        let b = Point3::from(vtx[1]);
        let c = Point3::from(vtx[2]);
        let n = (b - a).cross(&(c - a)).normalize();
        normals.push(n.cast::<f32>().into());
        normals.push(n.cast::<f32>().into());
        normals.push(n.cast::<f32>().into());
    }

    normals
        .iter_mut()
        .for_each(|n| *n = Vector3::from(*n).normalize().into());
    let indices: Vec<_> = (0..vertices.len() as u32).collect();
    let uvs: Vec<_> = (0..vertices.len()).map(|_| [0.0, 0.0]).collect();

    // Generate the mesh
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        VertexAttributeValues::from(vertices),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, VertexAttributeValues::from(normals));
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, VertexAttributeValues::from(uvs));
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(crate) fn collider_mesh_scale(co_shape: &dyn Shape) -> Vec3 {
    match co_shape.shape_type() {
        ShapeType::Ball => {
            let b = co_shape.as_ball().unwrap();
            Vec3::new(b.radius as f32, b.radius as f32, b.radius as f32)
        }
        ShapeType::Cuboid => {
            let c = co_shape.as_cuboid().unwrap();
            Vec3::from_slice(c.half_extents.cast::<f32>().as_slice())
        }
        ShapeType::RoundCuboid => {
            let c = co_shape.as_round_cuboid().unwrap();
            Vec3::from_slice(c.inner_shape.half_extents.cast::<f32>().as_slice())
        }
        ShapeType::Cylinder => {
            let c = co_shape.as_cylinder().unwrap();
            Vec3::new(c.radius as f32, c.half_height as f32, c.radius as f32)
        }
        ShapeType::RoundCylinder => {
            let c = &co_shape.as_round_cylinder().unwrap().inner_shape;
            Vec3::new(c.radius as f32, c.half_height as f32, c.radius as f32)
        }
        ShapeType::Cone => {
            let c = co_shape.as_cone().unwrap();
            Vec3::new(c.radius as f32, c.half_height as f32, c.radius as f32)
        }
        ShapeType::RoundCone => {
            let c = &co_shape.as_round_cone().unwrap().inner_shape;
            Vec3::new(c.radius as f32, c.half_height as f32, c.radius as f32)
        }
        _ => Vec3::ONE,
    }
}

pub(crate) fn generate_collider_mesh(co_shape: &dyn Shape) -> Option<Mesh> {
    let mesh = match co_shape.shape_type() {
        ShapeType::Capsule => {
            let capsule = co_shape.as_capsule().unwrap();
            bevy_mesh(capsule.to_trimesh(20, 10))
        }
        ShapeType::Triangle => {
            let tri = co_shape.as_triangle().unwrap();
            bevy_mesh((vec![tri.a, tri.b, tri.c], vec![[0, 1, 2], [0, 2, 1]]))
        }
        ShapeType::TriMesh => {
            let trimesh = co_shape.as_trimesh().unwrap();
            bevy_mesh((trimesh.vertices().to_vec(), trimesh.indices().to_vec()))
        }
        ShapeType::HeightField => {
            let heightfield = co_shape.as_heightfield().unwrap();
            bevy_mesh(heightfield.to_trimesh())
        }
        ShapeType::ConvexPolyhedron => {
            let poly = co_shape.as_convex_polyhedron().unwrap();
            bevy_mesh(poly.to_trimesh())
        }
        ShapeType::RoundConvexPolyhedron => {
            let poly = co_shape.as_round_convex_polyhedron().unwrap();
            bevy_mesh(poly.inner_shape.to_trimesh())
        }
        _ => return None,
    };

    Some(mesh)
}
