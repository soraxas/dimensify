use crate::objects::node::EntityWithGraphics;
use crate::BevyMaterial;
use bevy::asset::Assets;
use bevy::prelude::Mesh;
use bevy_ecs::prelude::Commands;

mod collider_as_entity;

pub use collider_as_entity::{ColliderAsMeshSpawner, ColliderAsMeshSpawnerBuilder};

const DEFAULT_OPACITY: f32 = 1.0;

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
