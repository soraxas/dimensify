use bevy::prelude::*;
use dimensify_protocol::{ProtoRequest, ProtoResponse};

#[cfg(feature = "transport")]
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::services::protocol_response::{
    draw::DrawCommand, pending_response::PendingApplyCommand,
};
use dimensify_protocol::WorldCommand;

use crate::{
    services::protocol_response::{
        list::handle_pending_request_list, pending_response::PendingRequestList,
    },
    stream::CommandLog,
};

pub fn plugin(app: &mut App) {
    app.add_systems(Update, handle_pending_request_list)
        .add_systems(
            Update,
            handle_transport_requests.before(handle_pending_request_list),
        );
}

/// The u64 is the entity id of the client.
/// TODO: align this with the actual client
#[derive(Component)]
struct ClientAdded(u64);

/// incoming requests from the client
fn handle_transport_requests(
    mut commands: Commands,
    mut command_log: ResMut<CommandLog>,
    // draw_commands: Query<Entity, With<DrawCommand>>,
    // mesh_entities: Query<Entity, With<Mesh3d>>,
    mut receivers: Populated<(
        Entity,
        &mut MessageReceiver<ProtoRequest>,
        &mut MessageSender<ProtoResponse>,
    )>,
) {
    for (entity, mut receiver, mut sender) in &mut receivers {
        for request in receiver.receive() {
            info!("Received request: {:?}", request);

            match request {
                ProtoRequest::ApplyCommand(command) => {
                    use bevy::log::info;
                    info!("Applying command: {:?}", command);

                    command_log.commands.push((entity, command));
                    // indicate that we need to send a response to the client after the command is applied
                    commands.entity(entity).insert(PendingApplyCommand);

                    // let _ = sender.send::<dimensify_transport::StreamReliable>(ProtoResponse::Ack);
                }
                ProtoRequest::List => {
                    // We need to wait for the response to be sent before we can remove the component.
                    commands.entity(entity).insert(PendingRequestList);
                }
            }
        }
    }
}
