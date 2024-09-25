use super::EntitySpawner;
use crate::constants::{DEFAULT_COLOR, DEFAULT_OPACITY};
use crate::graphics::InstancedMaterials;
use crate::objects::node;
use crate::objects::node::{ContainedEntity, EntityWithGraphics};
use crate::BevyMaterial;
use bevy::asset::{Assets, Handle};
use bevy::color::{Color, Srgba};
use bevy::math::Quat;
use bevy::prelude::{BuildChildren, Commands, Mesh, SpatialBundle, Transform};
use bevy_ecs::system::EntityCommands;
use bevy_pbr::wireframe::Wireframe;
use bevy_pbr::{PbrBundle, StandardMaterial};
use derive_builder::Builder;
use na::Point3;
use rapier3d::dynamics::{RigidBodyHandle, RigidBodySet};
use rapier3d::geometry::{Collider, ColliderHandle, ColliderSet, Cone, Cylinder, Shape, ShapeType};
use rapier3d::math::Isometry;
use rapier3d::prelude::{point, Real};
use std::collections::HashMap;

#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub struct ColliderAsMeshSpawner<'a> {
    pub handle: Option<ColliderHandle>,
    pub collider: &'a Collider,
    pub prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: &'a mut InstancedMaterials,
    #[builder(default = "Isometry::identity()")]
    pub delta: Isometry<Real>,

    #[builder(default = "DEFAULT_COLOR")]
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
        let mesh = node::bevy_mesh(cylinder.to_trimesh(20));
        out.insert(ShapeType::Cylinder, meshes.add(mesh.clone()));
        out.insert(ShapeType::RoundCylinder, meshes.add(mesh));

        //
        // Cone mesh
        //
        let cone = Cone::new(1.0, 1.0);
        let mesh = node::bevy_mesh(cone.to_trimesh(10));
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
        let mesh = node::bevy_mesh((vertices, indices));
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

        let scale = node::collider_mesh_scale(shape);
        let mesh = prefab_meshes
            .get(&shape.shape_type())
            .cloned()
            .or_else(|| node::generate_collider_mesh(shape).map(|m| meshes.add(m)));

        let bevy_color = Color::from(Srgba::new(color.x, color.y, color.z, DEFAULT_OPACITY));
        let shape_pos = collider_pos * delta;
        let mut transform = Transform::from_scale(scale);
        transform.translation.x = shape_pos.translation.vector.x;
        transform.translation.y = shape_pos.translation.vector.y;
        {
            transform.translation.z = shape_pos.translation.vector.z;
            transform.rotation = Quat::from_xyzw(
                shape_pos.rotation.i,
                shape_pos.rotation.j,
                shape_pos.rotation.k,
                shape_pos.rotation.w,
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

        EntityWithGraphics::new(
            entity_commands.id(),
            collider,
            delta,
            DEFAULT_OPACITY,
            ContainedEntity::Standalone {
                material: material_weak_handle,
            },
        )
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
            let scale = node::collider_mesh_scale(self.collider.shape());
            let shape_pos = self.collider.position() * self.delta;
            let transform = Transform {
                translation: shape_pos.translation.vector.into(),
                rotation: Quat::from_xyzw(
                    shape_pos.rotation.i,
                    shape_pos.rotation.j,
                    shape_pos.rotation.k,
                    shape_pos.rotation.w,
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
            EntityWithGraphics::new(
                parent_entity.id(),
                self.handle,
                self.delta,
                DEFAULT_OPACITY,
                ContainedEntity::Nested {
                    container: parent_entity.id(),
                    nested_children: children,
                },
            )
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
