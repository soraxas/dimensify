use crate::components::prelude::ProtoComponent;
use bevy_ecs::entity::Entity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtoRequest {
    ApplyCommand(WorldCommand),
    List,
}

#[cfg(feature = "bevy")]
use bevy::ecs::component::Component;

// #[cfg_attr(feature = "bevy", derive(Component))]

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub enum ProtoResponse {
    Ack,
    CommandResponseEntity(Entity),
    Entities { entities: Vec<EntityInfo> },
    Error { message: String },
}

// A wrapper type to re-pack a usize as a component id.
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
    pub id: u64,
    pub name: Option<String>,
    pub components: Vec<ComponentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub id: usize,
    pub name: String,
}

/// A command to be applied to the world.
///
/// They mimic the Bevy ECS API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorldCommand {
    Spawn {
        components: Vec<ProtoComponent>,
    },
    Insert {
        entity: Entity,
        components: Vec<ProtoComponent>,
    },
    Update {
        entity: Entity,
        component: ProtoComponent,
    },
    Remove {
        entity: Entity,
        component: ComponentId,
    },
    Despawn {
        entity: Entity,
    },
    Clear,
}
