use crate::scene_graphics::graphic_node::NodeWithGraphicsAndPhysics;
use crate::BevyMaterial;
use bevy::asset::{Assets, Handle};
use bevy::prelude::Mesh;
use bevy_ecs::prelude::Commands;

mod collider_as_entity;
pub mod spawn_from_datapack;

use crate::graphics::InstancedMaterials;
pub use collider_as_entity::*;
use rapier3d::dynamics::{ImpulseJointSet, MultibodyJointSet, RigidBodyHandle, RigidBodySet};
use rapier3d::geometry::{ColliderSet, ShapeType};
use std::collections::HashMap;

use super::prefab_mesh::PrefabMesh;

/// spawn one entity with graphics
pub trait EntitySpawner: Send + Sync {
    fn spawn(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
    ) -> NodeWithGraphicsAndPhysics;
}

/// A spawner that uses a closure to spawn an entity
impl<F> EntitySpawner for F
where
    F: FnMut(
        &mut Commands,
        &mut Assets<Mesh>,
        &mut Assets<BevyMaterial>,
    ) -> NodeWithGraphicsAndPhysics,
    F: Send + Sync,
{
    fn spawn(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
    ) -> NodeWithGraphicsAndPhysics {
        self(commands, meshes, materials)
    }
}

pub struct EntitySpawnerArg<'a, 'b, 'c> {
    pub commands: &'a mut Commands<'b, 'c>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<BevyMaterial>,
    pub bodies: &'a mut RigidBodySet,
    pub colliders: &'a mut ColliderSet,
    pub impulse_joints: &'a mut ImpulseJointSet,
    pub multibody_joints: &'a mut MultibodyJointSet,
    pub prefab_meshes: &'a mut PrefabMesh,
    pub instanced_materials: &'a mut InstancedMaterials,
}

/// spawn a set of entities, each associated to a rigid body
pub trait EntitySetSpawner: Send + Sync {
    fn spawn_entities_sets(
        &mut self,
        args: EntitySpawnerArg,
    ) -> HashMap<RigidBodyHandle, Vec<NodeWithGraphicsAndPhysics>>;
}

/// A spawner that uses a closure to spawn an entity
impl<F> EntitySetSpawner for F
where
    F: FnMut(EntitySpawnerArg) -> HashMap<RigidBodyHandle, Vec<NodeWithGraphicsAndPhysics>>,
    F: Send + Sync,
{
    fn spawn_entities_sets(
        &mut self,
        args: EntitySpawnerArg,
    ) -> HashMap<RigidBodyHandle, Vec<NodeWithGraphicsAndPhysics>> {
        self(args)
    }
}
