use crate::components::prelude::ProtoComponent;
use bevy_ecs::entity::Entity;
use serde::{Deserialize, Serialize};

#[cfg(feature = "bevy")]
use bevy::ecs::component::Component;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtoRequest {
    /// Apply a single world command.
    ApplyCommand(WorldCommand),
    /// List entities and their component ids.
    List,
}

#[cfg_attr(feature = "bevy", derive(Component))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtoResponse {
    /// Command applied successfully.
    Ack,
    /// Command returned a single entity (e.g., Spawn).
    CommandResponseEntity(Entity),
    /// Full entity listing for `ProtoRequest::List`.
    Entities { entities: Vec<EntityInfo> },
    /// Error response for malformed or failed requests.
    Error { message: String },
}

/// A wrapper type to re-pack a `usize` as a component id.
///
/// Use this with `WorldCommand::Remove` after retrieving ids from `List`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponentId(usize);

#[cfg(feature = "bevy")]
impl From<ComponentId> for bevy::ecs::component::ComponentId {
    fn from(id: ComponentId) -> Self {
        bevy::ecs::component::ComponentId::new(id.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInfo {
    /// Stable bits for the Bevy entity id.
    pub id: u64,
    /// Optional name component.
    pub name: Option<String>,
    /// All component ids for the entity.
    pub components: Vec<ComponentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Engine component id (use with `WorldCommand::Remove`).
    pub id: usize,
    /// Debug name for the component type.
    pub name: String,
}

/// A command to be applied to the world.
///
/// These mirror the Bevy ECS API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorldCommand {
    /// Spawn a new entity with the provided components.
    Spawn { components: Vec<ProtoComponent> },
    /// Insert components onto an existing entity.
    Insert {
        entity: Entity,
        components: Vec<ProtoComponent>,
    },
    /// Update a single component on an existing entity.
    Update {
        entity: Entity,
        component: ProtoComponent,
    },
    /// Remove a component by id from an existing entity.
    Remove {
        entity: Entity,
        component: ComponentId,
    },
    /// Despawn an entity.
    Despawn { entity: Entity },
    /// Clear the world (planned).
    Clear,
}
