#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};

//use crate::objects::plane::Plane;
use bevy::render::render_resource::PrimitiveTopology;
use na::{Point3, Vector3};
use rapier3d::geometry::{ColliderHandle, ColliderSet, Shape, ShapeType};
use rapier3d::math::{Isometry, Real, Vector};
use rapier3d::prelude::{ImpulseJointSet, MultibodyJointSet, RigidBodyHandle, RigidBodySet};
use std::collections::HashMap;

use crate::graphics::{BevyMaterial, InstancedMaterials};

pub trait EntitySpawnerBlahBlah: Send + Sync {
    fn spawn_with_sets(
        &mut self,
        args: EntitySpawnerArg,
    ) -> HashMap<RigidBodyHandle, Vec<EntityWithGraphics>>;
}

pub struct EntitySpawnerArg<'a, 'b, 'c> {
    pub commands: &'a mut Commands<'b, 'c>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<BevyMaterial>,
    pub bodies: &'a mut RigidBodySet,
    pub colliders: &'a mut ColliderSet,
    pub impulse_joints: &'a mut ImpulseJointSet,
    pub multibody_joints: &'a mut MultibodyJointSet,
    pub prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: &'a mut InstancedMaterials,
}

/// A spawner that uses a closure to spawn an entity
impl<F> EntitySpawnerBlahBlah for F
where
    F: FnMut(EntitySpawnerArg) -> HashMap<RigidBodyHandle, Vec<EntityWithGraphics>>,
    F: Send + Sync,
{
    fn spawn_with_sets(
        &mut self,
        args: EntitySpawnerArg,
    ) -> HashMap<RigidBodyHandle, Vec<EntityWithGraphics>> {
        self(args)
    }
}

#[derive(Clone, Debug)]
pub enum ContainedEntity {
    Nested {
        container: Entity,
        nested_children: Vec<EntityWithGraphics>,
    },
    Standalone {
        material: Handle<BevyMaterial>,
    },
}

#[derive(Clone, Debug)]
pub struct EntityWithGraphics {
    entity: Entity,
    pub collider: Option<ColliderHandle>,
    delta: Isometry<Real>,
    opacity: f32,
    value: ContainedEntity,
}

impl EntityWithGraphics {
    pub fn new(
        entity: Entity,
        collider: Option<ColliderHandle>,
        delta: Isometry<Real>,
        opacity: f32,
        value: ContainedEntity,
    ) -> Self {
        Self {
            entity,
            collider,
            delta,
            opacity,
            value,
        }
    }

    pub fn despawn(&mut self, commands: &mut Commands) {
        //FIXME: Should this be despawn_recursive?
        commands.entity(self.entity).despawn();
        self.visit_node_with_entity(&mut |_, entity| {
            commands.entity(entity).despawn();
        });
    }

    pub fn set_color(&mut self, materials: &mut Assets<BevyMaterial>, color: Point3<f32>) {
        match &mut self.value {
            ContainedEntity::Standalone { material } => {
                if let Some(material) = materials.get_mut(material) {
                    material.base_color =
                        Color::from(Srgba::new(color.x, color.y, color.z, self.opacity));
                }
            }
            &mut ContainedEntity::Nested { .. } => self.visit_node_mut(&mut |node| {
                node.set_color(materials, color);
            }),
        };
    }

    pub fn update(
        &mut self,
        colliders: &ColliderSet,
        components: &mut Query<&mut Transform>,
        gfx_shift: &Vector<Real>,
    ) {
        if let Some(Some(co)) = self.collider.map(|c| colliders.get(c)) {
            if let Ok(mut pos) = components.get_mut(self.entity) {
                let co_pos = co.position() * self.delta;
                pos.translation.x = (co_pos.translation.vector.x + gfx_shift.x) as f32;
                pos.translation.y = (co_pos.translation.vector.y + gfx_shift.y) as f32;
                {
                    pos.translation.z = (co_pos.translation.vector.z + gfx_shift.z) as f32;
                    pos.rotation = Quat::from_xyzw(
                        co_pos.rotation.i as f32,
                        co_pos.rotation.j as f32,
                        co_pos.rotation.k as f32,
                        co_pos.rotation.w as f32,
                    );
                }
            }
        }
    }

    /// a visitor pattern for the entity and its children
    pub fn visit_node_with_entity(&self, visitor: &mut impl FnMut(&EntityWithGraphics, Entity)) {
        match &self.value {
            ContainedEntity::Standalone { .. } => visitor(self, self.entity),
            ContainedEntity::Nested {
                nested_children, ..
            } => nested_children.iter().for_each(|c| visitor(c, c.entity)),
        };
    }

    /// a visitor pattern for the entity and its children
    pub fn visit_node(&self, visitor: &mut impl FnMut(&EntityWithGraphics)) {
        match &self.value {
            ContainedEntity::Standalone { .. } => visitor(self),
            ContainedEntity::Nested {
                nested_children, ..
            } => nested_children.iter().for_each(visitor),
        };
    }

    /// a visitor pattern for the entity and its children
    pub fn visit_node_mut(&mut self, visitor: &mut impl FnMut(&mut EntityWithGraphics)) {
        match &mut self.value {
            ContainedEntity::Standalone { .. } => visitor(self),
            ContainedEntity::Nested {
                nested_children, ..
            } => nested_children.iter_mut().for_each(visitor),
        };
    }

    pub fn get_material(&self) -> Option<&Handle<BevyMaterial>> {
        match &self.value {
            ContainedEntity::Standalone { material } => Some(material),
            ContainedEntity::Nested { .. } => None,
        }
    }
}

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
