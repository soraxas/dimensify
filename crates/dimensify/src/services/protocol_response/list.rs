use bevy::{ecs::component::Components, prelude::*};
use dimensify_protocol::ComponentInfo;
use dimensify_transport::{EntityInfo, ProtoResponse};
use lightyear::prelude::MessageSender;

use super::pending_response::{PendingRequestList, WithPendingResponse};
use bevy::{
    camera::Camera,
    window::{Monitor, Window},
};
use lightyear::prelude::{Client, Server};
// use bevy::picking::PointerLocation;

/// default filter for entities that are not needed to be listed by the transport client.
type DefaultEntityFilter = (
    Without<MessageSender<ProtoResponse>>,
    // bevy window
    Without<Window>,
    Without<Monitor>,
    // Without<PointerLocation>,
    // lightyear server
    Without<Server>,
    Without<Client>,
    // bevy visual
    Without<DirectionalLight>,
    Without<Camera>,
);

/// These are all queued list requests that need to be processed.
pub(crate) fn handle_pending_request_list(
    mut commands: Commands,
    q_entities: Query<(Entity, EntityRef, Option<&Name>), DefaultEntityFilter>,
    mut senders_with_pending_reqs: WithPendingResponse<With<PendingRequestList>>,
    components: &Components,
) {
    for (entity, mut sender) in &mut senders_with_pending_reqs {
        commands.entity(entity).remove::<PendingRequestList>();
        // Send the list of entities to the client.

        let mut entities_out = Vec::new();
        for (entity, entity_ref, name) in &q_entities {
            let components = entity_ref
                .archetype()
                .components()
                .iter()
                .filter_map(|id| {
                    components.get_info(*id).map(|info| ComponentInfo {
                        id: info.id().index(),
                        name: info.name().to_string(),
                    })
                })
                .collect::<Vec<ComponentInfo>>();

            entities_out.push(EntityInfo {
                id: entity.to_bits(),
                name: name.map(|s| s.to_string()),
                components,
            });
        }
        let _ = sender.send::<dimensify_transport::StreamReliable>(ProtoResponse::Entities {
            entities: entities_out,
        });
    }
}
