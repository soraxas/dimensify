//! Shared room authority behind WebSocket and WebTransport listeners.
use crate::{authority::Authority, protocol::*, *};
use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::transport::channel::registry::ChannelKind;
pub use lightyear::webtransport::prelude::Identity as WebTransportIdentity;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
#[derive(Resource)]
pub struct ServerLimits {
    pub connections: usize,
    pub join_timeout: Duration,
    pub requests_per_second: usize,
    pub pending_controls: usize,
}
impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            connections: 128,
            join_timeout: Duration::from_secs(15),
            requests_per_second: 120,
            pending_controls: 64,
        }
    }
}
#[derive(Resource, Default)]
struct Rooms<D: SharedDocument>(Authority<D>);
#[derive(Default)]
struct Peer {
    was_connected: bool,
    joined_at: Duration,
    window: u64,
    requests: usize,
    pending: usize,
    last_ack: Duration,
    last_received: Duration,
}
#[derive(Resource, Default)]
struct Peers(BTreeMap<Entity, Peer>);
pub use dimensify_transport::server::{
    Listener as CollaborationListener, listen, listen_websocket, listen_webtransport,
};
use dimensify_transport::server::{TransportPeer as CollaborationPeer, TransportServerPlugin};
#[derive(Default)]
pub struct CollaborationServerPlugin<D: SharedDocument>(std::marker::PhantomData<D>);
impl<D: SharedDocument> Plugin for CollaborationServerPlugin<D> {
    fn build(&self, app: &mut App) {
        app.add_plugins((TransportServerPlugin, ProtocolPlugin::<D>::default()))
            .init_resource::<Rooms<D>>()
            .init_resource::<Peers>()
            .init_resource::<ServerLimits>()
            .add_observer(new_peer::<D>)
            .add_systems(Update, pump_server::<D>);
    }
}
fn new_peer<D: SharedDocument>(event: On<Add, CollaborationPeer>, mut commands: Commands) {
    commands.entity(event.entity).insert((
        MessageReceiver::<ClientEnvelope<D>>::default(),
        MessageSender::<ServerEnvelope<D>>::default(),
    ));
}
#[allow(
    clippy::type_complexity,
    reason = "Keep the optional transport components explicit at the ECS system boundary"
)]
fn pump_server<D: SharedDocument>(
    mut commands: Commands,
    time: Res<Time<Real>>,
    limits: Res<ServerLimits>,
    mut rooms: ResMut<Rooms<D>>,
    mut peers: ResMut<Peers>,
    mut query: Query<
        (
            Entity,
            Option<&mut MessageReceiver<ClientEnvelope<D>>>,
            &mut MessageSender<ServerEnvelope<D>>,
            Option<&Transport>,
            Option<&Connected>,
            Option<&Unlinked>,
        ),
        With<CollaborationPeer>,
    >,
) {
    let now = time.elapsed();
    let seconds = now.as_secs();
    let mut outgoing = Vec::new();
    let alive: BTreeSet<_> = query.iter().map(|(e, _, _, _, _, _)| e).collect();
    let removed: Vec<_> = peers
        .0
        .keys()
        .filter(|e| !alive.contains(e))
        .copied()
        .collect();
    for entity in removed {
        peers.0.remove(&entity);
        outgoing.extend(rooms.0.disconnect(entity.to_bits(), seconds));
    }
    let mut close = BTreeSet::new();
    for (entity, mut receiver, _, transport, connected, unlinked) in &mut query {
        if !peers.0.contains_key(&entity) && peers.0.len() >= limits.connections {
            close.insert(entity);
            continue;
        }
        let peer = peers.0.entry(entity).or_insert_with(|| Peer {
            joined_at: now,
            last_ack: now,
            last_received: now,
            ..default()
        });
        if let Some(channel) =
            transport.and_then(|t| t.channel_send(ChannelKind::of::<ControlChannel>()))
        {
            let count = channel.message_acks().len();
            if count > 0 {
                peer.pending = peer.pending.saturating_sub(count);
                peer.last_ack = now;
            }
        }
        if (peer.was_connected && connected.is_none())
            || unlinked.is_some()
            || (!rooms.0.is_joined(entity.to_bits())
                && now.saturating_sub(peer.joined_at) > limits.join_timeout)
            || (rooms.0.is_joined(entity.to_bits())
                && now.saturating_sub(peer.last_received) > Duration::from_secs(30))
            || (peer.pending > 0 && now.saturating_sub(peer.last_ack) > Duration::from_secs(15))
        {
            close.insert(entity);
            continue;
        }
        if connected.is_none() {
            continue;
        }
        peer.was_connected = true;
        if peer.window != seconds {
            peer.window = seconds;
            peer.requests = 0;
        }
        for request in receiver.as_mut().into_iter().flat_map(|r| r.receive()) {
            peer.last_received = now;
            peer.requests += 1;
            if peer.requests > limits.requests_per_second {
                close.insert(entity);
                break;
            }
            outgoing.extend(process_request(
                &mut rooms.0,
                entity.to_bits(),
                request.message,
                seconds,
            ));
        }
    }
    // Coalesce camera and room snapshots per peer within this update; preserve Welcome/errors.
    let mut latest_cursor = BTreeMap::new();
    let mut latest_camera = BTreeMap::new();
    let mut latest_view = BTreeMap::new();
    let mut latest_state = BTreeMap::new();
    let mut controls = Vec::new();
    for (peer, message) in outgoing {
        match message {
            ServerMessage::Cursor(ref cursor) => {
                if rooms.0.accepts_cursor(peer, cursor) {
                    // Subscription retirement must survive packet loss. Ordinary
                    // latest-value presence remains on the unreliable lane.
                    if cursor.sample.sequence == u64::MAX
                        && cursor.sample.position.is_none()
                        && cursor.sample.spatial.is_none()
                    {
                        controls.push((peer, message));
                    } else {
                        latest_cursor.insert((peer, cursor.participant), message);
                    }
                }
            }
            ServerMessage::View { participant, .. } => {
                latest_view.insert((peer, participant), message);
            }
            ServerMessage::Camera(_) => {
                latest_camera.insert(peer, message);
            }
            ServerMessage::Snapshot(_) => {
                latest_state.insert(peer, message);
            }
            _ => controls.push((peer, message)),
        }
    }
    controls.extend(latest_state);
    for (id, message) in controls {
        let Some(entity) = alive.iter().find(|e| e.to_bits() == id).copied() else {
            continue;
        };
        let Some(peer) = peers.0.get_mut(&entity) else {
            continue;
        };
        if peer.pending >= limits.pending_controls {
            close.insert(entity);
            continue;
        }
        if let Ok((_, _, mut sender, _, _, _)) = query.get_mut(entity) {
            if peer.pending == 0 {
                peer.last_ack = now;
            }
            peer.pending += 1;
            sender.send::<ControlChannel>(Envelope::new(message));
        }
    }
    for (id, message) in latest_camera {
        if let Some(entity) = alive.iter().find(|e| e.to_bits() == id)
            && let Ok((_, _, mut sender, _, _, _)) = query.get_mut(*entity)
        {
            sender.send::<CameraChannel>(Envelope::new(message));
        }
    }
    for ((id, _), message) in latest_cursor.into_iter().chain(latest_view) {
        if let Some(entity) = alive.iter().find(|e| e.to_bits() == id)
            && let Ok((_, _, mut sender, _, _, _)) = query.get_mut(*entity)
        {
            sender.send::<CursorChannel>(Envelope::new(message));
        }
    }
    for entity in close {
        commands.trigger(Disconnect { entity });
        commands.entity(entity).despawn();
    }
    rooms.0.expire(seconds);
}

fn process_request<D: SharedDocument>(
    authority: &mut Authority<D>,
    peer: u64,
    request: ClientRequest<D>,
    now: u64,
) -> Vec<(u64, ServerMessage<D>)> {
    // Presence and latest-value samples can legitimately lose their context at
    // handoff. Their rejection must not cancel unrelated queued reliable controls;
    // the current room snapshot lets the adapter publish fresh presence instead.
    let presence = matches!(
        &request,
        ClientRequest::PublishCamera(_)
            | ClientRequest::PublishView(_)
            | ClientRequest::PublishCursor(_)
            | ClientRequest::SetCursorSubscription(_)
            | ClientRequest::SetFollowing { .. }
    );
    let decision = match &request {
        ClientRequest::GrantPresenter { request } | ClientRequest::DismissPresenter { request } => {
            Some(*request)
        }
        _ => None,
    };
    match authority.process(peer, request, now) {
        Ok(messages) => messages,
        Err(_) if presence => vec![],
        Err(error) => vec![(
            peer,
            match decision {
                Some(request) => ServerMessage::PresenterDecisionRejected { request, error },
                None => ServerMessage::Rejected(error),
            },
        )],
    }
}

#[cfg(all(test, feature = "client"))]
mod tests {
    use super::*;
    #[test]
    fn rejected_decisions_keep_request_identity_and_do_not_cancel_other_controls() {
        let mut authority = Authority::<crate::protocol::tests::TestDocument>::default();
        let request = PresenterRequest {
            participant: 2,
            id: 3,
        };
        let messages = process_request(
            &mut authority,
            9,
            ClientRequest::DismissPresenter { request },
            0,
        );
        assert_eq!(
            messages,
            vec![(
                9,
                ServerMessage::PresenterDecisionRejected {
                    request,
                    error: SessionError::NotJoined
                }
            )]
        );
        let mut session = ClientSession::<crate::protocol::tests::TestDocument>::default();
        session.status = ConnectionStatus::Connected;
        session.send(ClientRequest::RequestPresenter).unwrap();
        session.receive(messages[0].1.clone());
        assert_eq!(
            session.pending_count(),
            1,
            "correlated rejection does not discard unrelated controls"
        );
    }

    #[test]
    fn rejected_camera_does_not_cancel_queued_control_but_control_errors_are_explicit() {
        let mut authority = Authority::<crate::protocol::tests::TestDocument>::default();
        let created = authority
            .process(
                1,
                ClientRequest::Create {
                    name: "host".into(),
                },
                0,
            )
            .unwrap();
        let ServerMessage::Welcome {
            snapshot,
            invitations: Some(invites),
            ..
        } = created[0].1.clone()
        else {
            panic!()
        };
        authority
            .process(
                2,
                ClientRequest::Join {
                    room: snapshot.room.clone(),
                    invitation: invites.viewer,
                    name: "viewer".into(),
                },
                0,
            )
            .unwrap();
        authority
            .process(2, ClientRequest::RequestPresenter, 0)
            .unwrap();
        authority
            .process(
                1,
                ClientRequest::GrantPresenter {
                    request: authority
                        .rooms
                        .values()
                        .next()
                        .unwrap()
                        .state
                        .presenter_requests[0],
                },
                0,
            )
            .unwrap();
        let mut client = ClientSession::default();
        client.receive(created[0].1.clone());
        client.send(ClientRequest::RequestPresenter).unwrap();
        let camera = CameraSnapshot {
            room: snapshot.room.clone(),
            epoch: 1,
            scene_revision: 0,
            sequence: 1,
            pose: CameraPose {
                position: [0.; 3],
                rotation: [0., 0., 0., 1.],
                projection: crate::wire::Projection::Perspective { vertical_fov: 1. },
                focus_distance: 1.,
            },
        };
        assert_eq!(
            authority.process(1, ClientRequest::PublishCamera(camera.clone()), 0),
            Err(SessionError::Unauthorized)
        );
        let replies = process_request(&mut authority, 1, ClientRequest::PublishCamera(camera), 0);
        for (_, message) in replies {
            client.receive(message);
        }
        assert_eq!(
            client.pending_count(),
            1,
            "late camera rejection must not cancel a queued presenter request"
        );
        assert!(client.error.is_none());
        for (_, message) in process_request(
            &mut authority,
            1,
            ClientRequest::SetFollowing {
                room: snapshot.room.clone(),
                presenter: snapshot.presenter,
                epoch: snapshot.camera_epoch,
                target: snapshot.presenter,
            },
            0,
        ) {
            client.receive(message);
        }
        assert_eq!(
            client.pending_count(),
            1,
            "late follow presence must not cancel a queued presenter request"
        );
        assert!(client.error.is_none());
        let replies = process_request(
            &mut authority,
            2,
            ClientRequest::SetDocument {
                expected_revision: 0,
                data: Default::default(),
            },
            0,
        );
        assert_eq!(
            replies,
            vec![(2, ServerMessage::Rejected(SessionError::Unauthorized))]
        );
    }
}
