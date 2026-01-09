use bevy::{ecs::component::Components, prelude::*};
use dimensify_transport::{ViewerEntityInfo, ViewerResponse};
use lightyear::prelude::MessageSender;

use bevy::{
    camera::Camera,
    window::{Monitor, Window},
};
use lightyear::prelude::{Client, Server};
// use bevy::picking::PointerLocation;

/// A marker component for entities that are waiting for a list request.
#[derive(Component)]
pub(crate) struct PendingRequestList;

/// default filter for entities that are not needed to be listed by the transport client.
type DefaultEntityFilter = (
    Without<MessageSender<ViewerResponse>>,
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
    mut senders_with_pending_reqs: Populated<
        (Entity, &mut MessageSender<ViewerResponse>),
        (
            With<PendingRequestList>,
            With<MessageSender<ViewerResponse>>,
        ),
    >,
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
                .filter_map(|id| components.get_info(*id).map(|info| info.name().to_string()))
                .collect::<Vec<String>>();

            entities_out.push(ViewerEntityInfo {
                id: entity.to_bits(),
                name: name.map(|s| s.to_string()),
                components,
            });
        }
        let _ = sender.send::<dimensify_transport::StreamReliable>(ViewerResponse::Entities {
            entities: entities_out,
        });
    }
}
