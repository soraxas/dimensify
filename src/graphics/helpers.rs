#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::prelude::*;

use bevy::render::mesh::{Indices, VertexAttributeValues};
use bevy::render::render_resource::PrimitiveTopology;
use rapier3d::geometry::{Shape, ShapeType};
use rapier3d::math::Real;
use rapier3d::na::{Point3, Vector3};

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

const NTHETA_SUBDIV: u32 = 20;
const NPHI_SUBDIV: u32 = 10;

pub fn generate_collider_mesh(co_shape: &dyn Shape) -> Option<Mesh> {
    let mesh = match co_shape.shape_type() {
        ShapeType::Capsule => {
            let capsule = co_shape.as_capsule().unwrap();
            bevy_mesh(capsule.to_trimesh(NTHETA_SUBDIV, NPHI_SUBDIV))
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
        ShapeType::Ball => {
            let ball = co_shape.as_ball().unwrap();
            bevy_mesh(ball.to_trimesh(NTHETA_SUBDIV, NPHI_SUBDIV))
        }
        ShapeType::Cuboid => {
            let cuboid = co_shape.as_cuboid().unwrap();
            bevy_mesh(cuboid.to_trimesh())
        }
        ShapeType::Segment => {
            let segment = co_shape.as_segment().unwrap();
            todo!();
        }
        ShapeType::Polyline => {
            let polyline = co_shape.as_polyline().unwrap();
            todo!();
        }
        // ShapeType::HalfSpace => {

        // },
        ShapeType::Compound => todo!(),
        ShapeType::Cylinder => {
            let cylinder = co_shape.as_cylinder().unwrap();
            bevy_mesh(cylinder.to_trimesh(NTHETA_SUBDIV))
        }
        ShapeType::Cone => todo!(),
        ShapeType::RoundCuboid => todo!(),
        ShapeType::RoundTriangle => todo!(),
        ShapeType::RoundCylinder => {
            let cylinder = co_shape.as_round_cylinder().unwrap();
            // bevy_mesh(cylinder.to_trimesh(NTHETA_SUBDIV))
            todo!()
        }
        ShapeType::RoundCone => todo!(),
        ShapeType::Custom => todo!(),
        ShapeType::HalfSpace => {
            let halfspace = co_shape.as_halfspace().unwrap();
            let vertices = vec![
                Point3::new(-1000.0, 0.0, -1000.0),
                Point3::new(1000.0, 0.0, -1000.0),
                Point3::new(1000.0, 0.0, 1000.0),
                Point3::new(-1000.0, 0.0, 1000.0),
            ];
            let indices = vec![[0, 1, 2], [0, 2, 3]];
            bevy_mesh((vertices, indices))
        }
        _ => {
            todo!(
                "The given shape {:#?} is not supported by the mesh generator.",
                co_shape.shape_type()
            );
            return None;
        }
    };

    Some(mesh)
}
