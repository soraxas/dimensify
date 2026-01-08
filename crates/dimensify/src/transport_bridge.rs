use bevy::prelude::*;
use dimensify_transport::{ViewerRequest, ViewerResponse};

#[cfg(feature = "transport")]
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{stream::CommandLog, viewer::SceneEntities};
use dimensify_protocol::{Command, commands::DrawCommand};

use crate::protocol_response::list::{PendingRequestList, handle_pending_request_list};

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (handle_transport_requests, handle_pending_request_list),
    );
}

/// The u64 is the entity id of the client.
/// TODO: align this with the actual client
#[derive(Component)]
struct ClientAdded(u64);

fn handle_transport_requests(
    mut commands: Commands,
    mut command_log: ResMut<CommandLog>,
    mut entities: ResMut<SceneEntities>,
    draw_commands: Query<Entity, With<DrawCommand>>,
    mut receivers: Populated<(
        Entity,
        &mut MessageReceiver<ViewerRequest>,
        &mut MessageSender<ViewerResponse>,
    )>,
) {
    for (entity, mut receiver, mut sender) in &mut receivers {
        for request in receiver.receive() {
            match request {
                ViewerRequest::ApplyJson { payload } => match decode_commands(payload.as_bytes()) {
                    Ok(mut commands_in) => {
                        command_log.commands.append(&mut commands_in);
                        let _ =
                            sender.send::<dimensify_transport::StreamReliable>(ViewerResponse::Ack);
                    }
                    Err(message) => {
                        let _ = sender.send::<dimensify_transport::StreamReliable>(
                            ViewerResponse::Error { message },
                        );
                    }
                },
                ViewerRequest::Remove { name } => {
                    if let Some(entity) = entities.map.remove(&name) {
                        commands.entity(entity).despawn();
                        let _ =
                            sender.send::<dimensify_transport::StreamReliable>(ViewerResponse::Ack);
                    } else {
                        let _ = sender.send::<dimensify_transport::StreamReliable>(
                            ViewerResponse::Error {
                                message: format!("unknown entity '{}'", name),
                            },
                        );
                    }
                }
                ViewerRequest::List => {
                    // We need to wait for the response to be sent before we can remove the component.
                    commands.entity(entity).insert(PendingRequestList);
                }
                ViewerRequest::Clear => {
                    command_log.commands.clear();
                    entities.map.clear();
                    for entity in &draw_commands {
                        commands.entity(entity).despawn();
                    }
                    let _ = sender.send::<dimensify_transport::StreamReliable>(ViewerResponse::Ack);
                }
            }
        }
    }
}

fn decode_commands(payload: &[u8]) -> Result<Vec<Command>, String> {
    if payload.is_empty() {
        return Ok(Vec::new());
    }
    if payload.first() == Some(&b'[') {
        serde_json::from_slice::<Vec<Command>>(payload)
            .map_err(|err| format!("command decode failed: {}", err))
    } else {
        serde_json::from_slice::<Command>(payload)
            .map(|command| vec![command])
            .map_err(|err| format!("command decode failed: {}", err))
    }
}
