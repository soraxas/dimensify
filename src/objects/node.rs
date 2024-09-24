#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};

use bevy_ecs::system::EntityCommands;
use derive_builder::Builder;
//use crate::objects::plane::Plane;
use na::{point, Point3, Vector3};
use rapier3d::prelude::{
    Collider, ColliderBuilder, Compound, ImpulseJointSet, MultibodyJointSet, RigidBodyHandle,
    RigidBodySet,
};
use std::collections::HashMap;
use std::option::Iter;

use bevy::render::render_resource::PrimitiveTopology;
use bevy_pbr::wireframe::Wireframe;
use rapier3d::geometry::{ColliderHandle, ColliderSet, Shape, ShapeType};
use rapier3d::geometry::{Cone, Cylinder};
use rapier3d::math::{Isometry, Real, Vector};

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

pub trait EntitySpawner: Send + Sync {
    fn spawn(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
    ) -> EntityWithGraphics;
}

/// A spawner that uses a closure to spawn an entity
impl<F> EntitySpawner for F
where
    F: FnMut(&mut Commands, &mut Assets<Mesh>, &mut Assets<BevyMaterial>) -> EntityWithGraphics,
    F: Send + Sync,
{
    fn spawn(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
    ) -> EntityWithGraphics {
        self(commands, meshes, materials)
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

const DEFAULT_OPACITY: f32 = 1.0;

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

#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub struct ColliderAsMeshSpawner<'a> {
    pub handle: Option<ColliderHandle>,
    pub collider: &'a Collider,
    pub prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: &'a mut InstancedMaterials,
    #[builder(default = "Isometry::identity()")]
    pub delta: Isometry<Real>,

    #[builder(default = "point![0.5, 0.5, 0.5]")]
    pub color: Point3<f32>,
}

impl<'a> ColliderAsMeshSpawner<'a> {
    pub fn builder_from_collider_builder(
        collider: impl Into<Collider>,
        body_handle: RigidBodyHandle,
        colliders: &'a mut ColliderSet,
        bodies: &'a mut RigidBodySet,
        prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
        instanced_materials: &'a mut InstancedMaterials,
    ) -> ColliderAsMeshSpawnerBuilder<'a> {
        let handler = colliders.insert_with_parent(collider, body_handle, bodies);

        ColliderAsMeshSpawnerBuilder::default()
            .handle(Some(handler))
            .collider(&colliders[handler])
            .prefab_meshes(prefab_meshes)
            .instanced_materials(instanced_materials)
    }

    pub fn gen_prefab_meshes(
        out: &mut HashMap<ShapeType, Handle<Mesh>>,
        meshes: &mut Assets<Mesh>,
    ) {
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

    fn spawn_child(
        entity_commands: &mut EntityCommands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        prefab_meshes: &HashMap<ShapeType, Handle<Mesh>>,
        instanced_materials: &mut InstancedMaterials,
        shape: &dyn Shape,
        collider: Option<ColliderHandle>,
        collider_pos: Isometry<Real>,
        delta: Isometry<Real>,
        color: Point3<f32>,
        sensor: bool,
    ) -> EntityWithGraphics {
        // Self::register_selected_object_material(materials, instanced_materials);

        let scale = collider_mesh_scale(shape);
        let mesh = prefab_meshes
            .get(&shape.shape_type())
            .cloned()
            .or_else(|| generate_collider_mesh(shape).map(|m| meshes.add(m)));

        let bevy_color = Color::from(Srgba::new(color.x, color.y, color.z, DEFAULT_OPACITY));
        let shape_pos = collider_pos * delta;
        let mut transform = Transform::from_scale(scale);
        transform.translation.x = shape_pos.translation.vector.x as f32;
        transform.translation.y = shape_pos.translation.vector.y as f32;
        {
            transform.translation.z = shape_pos.translation.vector.z as f32;
            transform.rotation = Quat::from_xyzw(
                shape_pos.rotation.i as f32,
                shape_pos.rotation.j as f32,
                shape_pos.rotation.k as f32,
                shape_pos.rotation.w as f32,
            );
        }
        let material = StandardMaterial {
            metallic: 0.5,
            perceptual_roughness: 0.5,
            double_sided: true, // TODO: this doesn't do anything?
            ..StandardMaterial::from(bevy_color)
        };
        let material_handle = instanced_materials
            .entry(color.coords.map(|c| (c * 255.0) as usize).into())
            .or_insert_with(|| materials.add(material));
        let material_weak_handle = material_handle.clone_weak();

        if let Some(mesh) = mesh {
            let bundle = PbrBundle {
                mesh,
                material: material_handle.clone_weak(),
                transform,
                ..Default::default()
            };

            entity_commands.insert(bundle);

            if sensor {
                entity_commands.insert(Wireframe);
            }
        }

        EntityWithGraphics {
            entity: entity_commands.id(),
            collider,
            delta,
            opacity: DEFAULT_OPACITY,
            value: ContainedEntity::Standalone {
                material: material_weak_handle,
            },
        }
    }
}

impl<'a> EntitySpawner for ColliderAsMeshSpawner<'a> {
    fn spawn(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
    ) -> EntityWithGraphics {
        if self.prefab_meshes.is_empty() {
            Self::gen_prefab_meshes(self.prefab_meshes, meshes);
        }

        if let Some(compound) = self.collider.shape().as_compound() {
            let scale = collider_mesh_scale(self.collider.shape());
            let shape_pos = self.collider.position() * self.delta;
            let transform = Transform {
                translation: shape_pos.translation.vector.into(),
                rotation: Quat::from_xyzw(
                    shape_pos.rotation.i as f32,
                    shape_pos.rotation.j as f32,
                    shape_pos.rotation.k as f32,
                    shape_pos.rotation.w as f32,
                ),
                scale,
            };

            let mut parent_entity = commands.spawn(SpatialBundle::from_transform(transform));

            let mut children: Vec<EntityWithGraphics> = Vec::new();
            parent_entity.with_children(|child_builder| {
                for (shape_pos, shape) in compound.shapes() {
                    // recursively add all shapes in the compound

                    let child_entity = &mut child_builder.spawn_empty();

                    // we don't need to add children directly to the vec, as all operation will be transitive
                    children.push(Self::spawn_child(
                        child_entity,
                        meshes,
                        materials,
                        self.prefab_meshes,
                        self.instanced_materials,
                        &**shape,
                        self.handle,
                        *shape_pos,
                        self.delta,
                        self.color,
                        self.collider.is_sensor(),
                    ));
                }
            });
            EntityWithGraphics {
                entity: parent_entity.id(),
                collider: self.handle,
                delta: self.delta,
                opacity: DEFAULT_OPACITY,
                value: ContainedEntity::Nested {
                    container: parent_entity.id(),
                    nested_children: children,
                },
            }
        } else {
            ColliderAsMeshSpawner::spawn_child(
                &mut commands.spawn_empty(),
                meshes,
                materials,
                self.prefab_meshes,
                self.instanced_materials,
                self.collider.shape(),
                self.handle,
                *self.collider.position(),
                self.delta,
                self.color,
                self.collider.is_sensor(),
            )
        }
    }
}

fn bevy_mesh(buffers: (Vec<Point3<Real>>, Vec<[u32; 3]>)) -> Mesh {
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

fn generate_collider_mesh(co_shape: &dyn Shape) -> Option<Mesh> {
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
