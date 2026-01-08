use bevy::prelude::*;
use dimensify_protocol::{SceneRequest, ViewerResponse};

#[cfg(feature = "transport")]
use lightyear::prelude::{MessageReceiver, MessageSender};

use crate::{draw_command::DrawCommand, stream::CommandLog, viewer::SceneEntities};
use dimensify_protocol::SceneCommand;

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
    mesh_entities: Query<Entity, With<Mesh3d>>,
    mut receivers: Populated<(
        Entity,
        &mut MessageReceiver<SceneRequest>,
        &mut MessageSender<ViewerResponse>,
    )>,
) {
    for (entity, mut receiver, mut sender) in &mut receivers {
        for request in receiver.receive() {
            match request {
                SceneRequest::Apply { payload } => {
                    match decode_scene_commands(payload.as_bytes()) {
                        Ok(mut commands_in) => {
                            command_log.commands.append(&mut commands_in);
                            let _ = sender
                                .send::<dimensify_transport::StreamReliable>(ViewerResponse::Ack);
                        }
                        Err(message) => {
                            let _ = sender.send::<dimensify_transport::StreamReliable>(
                                ViewerResponse::Error { message },
                            );
                        }
                    }
                }
                SceneRequest::Remove { name } => {
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
                SceneRequest::List => {
                    // We need to wait for the response to be sent before we can remove the component.
                    commands.entity(entity).insert(PendingRequestList);
                }
                SceneRequest::Clear => {
                    command_log.commands.clear();
                    entities.map.clear();
                    for entity in &draw_commands {
                        commands.entity(entity).despawn();
                    }
                    for entity in &mesh_entities {
                        commands.entity(entity).despawn();
                    }
                    let _ = sender.send::<dimensify_transport::StreamReliable>(ViewerResponse::Ack);
                }
            }
        }
    }
}

fn decode_scene_commands(payload: &[u8]) -> Result<Vec<SceneCommand>, String> {
    if payload.is_empty() {
        return Ok(Vec::new());
    }
    if payload.first() == Some(&b'[') {
        serde_json::from_slice::<Vec<SceneCommand>>(payload)
            .map_err(|err| format!("command decode failed: {}", err))
    } else {
        serde_json::from_slice::<SceneCommand>(payload)
            .map(|command| vec![command])
            .map_err(|err| format!("command decode failed: {}", err))
    }
}
