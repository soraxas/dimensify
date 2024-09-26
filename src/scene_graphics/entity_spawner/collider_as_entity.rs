use super::EntitySpawner;
use crate::constants::{DEFAULT_COLOR, DEFAULT_OPACITY};
use crate::graphics::InstancedMaterials;
use crate::scene::node::NodeInner;
use crate::scene_graphics::graphic_node::{
    NodeDataGraphics, NodeWithGraphics, NodeWithGraphicsBuilder,
};
use crate::scene_graphics::helpers::{bevy_mesh, collider_mesh_scale, generate_collider_mesh};
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

use super::helpers::gen_prefab_meshes;

#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub struct ColliderAsPrefabMeshWithPhysicsSpawner<'a> {
    pub handle: Option<ColliderHandle>,
    pub collider: &'a Collider,
    pub prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: &'a mut InstancedMaterials,
    #[builder(default = "Isometry::identity()")]
    pub delta: Isometry<Real>,

    #[builder(default = "DEFAULT_COLOR")]
    pub color: Point3<f32>,
}

impl<'a> ColliderAsPrefabMeshWithPhysicsSpawner<'a> {
    pub fn builder_from_collider_builder(
        collider: impl Into<Collider>,
        body_handle: RigidBodyHandle,
        colliders: &'a mut ColliderSet,
        bodies: &'a mut RigidBodySet,
        prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
        instanced_materials: &'a mut InstancedMaterials,
    ) -> ColliderAsPrefabMeshWithPhysicsSpawnerBuilder<'a> {
        let handler = colliders.insert_with_parent(collider, body_handle, bodies);

        ColliderAsPrefabMeshWithPhysicsSpawnerBuilder::default()
            .handle(Some(handler))
            .collider(&colliders[handler])
            .prefab_meshes(prefab_meshes)
            .instanced_materials(instanced_materials)
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
    ) -> NodeWithGraphics {
        // Self::register_selected_object_material(materials, instanced_materials);

        let scale = collider_mesh_scale(shape);
        let mesh = prefab_meshes
            .get(&shape.shape_type())
            .cloned()
            .or_else(|| generate_collider_mesh(shape).map(|m| meshes.add(m)));

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

        NodeWithGraphicsBuilder::default()
            .collider(collider)
            .delta(delta)
            .data(NodeDataGraphics {
                entity: Some(entity_commands.id()),
                opacity: DEFAULT_OPACITY,
            })
            .value(material_weak_handle.into())
            .build()
            .expect("All fields are set")
    }
}

impl<'a> EntitySpawner for ColliderAsPrefabMeshWithPhysicsSpawner<'a> {
    fn spawn(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
    ) -> NodeWithGraphics {
        if self.prefab_meshes.is_empty() {
            gen_prefab_meshes(self.prefab_meshes, meshes);
        }

        if let Some(compound) = self.collider.shape().as_compound() {
            let scale = collider_mesh_scale(self.collider.shape());
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

            let mut children: Vec<NodeWithGraphics> = Vec::new();
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

            NodeWithGraphicsBuilder::default()
                .delta(self.delta)
                .collider(self.handle)
                .data(NodeDataGraphics {
                    entity: Some(parent_entity.id()),
                    opacity: DEFAULT_OPACITY,
                })
                .value(NodeInner::Nested { children })
                .build()
                .expect("All fields are set")
        } else {
            Self::spawn_child(
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
