//! Listener lifecycle and peer routing for one logical raw server.
use bevy::prelude::*;
use lightyear::prelude::{server::*, *};
pub use lightyear::webtransport::prelude::Identity as WebTransportIdentity;
use std::net::SocketAddr;
/// A transport endpoint belonging to the application's single logical server.
/// Observe `Linked`/`Unlinked` on the entity returned by `listen*` for readiness.
#[derive(Component)]
pub struct Listener {
    server: Entity,
}
#[derive(Component)]
struct TransportAuthority;
#[derive(Component)]
pub struct TransportPeer;

/// Install transport listeners and their shared logical server without room policy.
#[derive(Default)]
pub struct TransportServerPlugin;
impl Plugin for TransportServerPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = app
            .world()
            .get_resource::<Time<Fixed>>()
            .map_or_else(|| Time::<Fixed>::default().timestep(), Time::timestep);
        app.add_plugins(ServerPlugins { tick_duration })
            .add_observer(new_peer)
            .add_observer(stop_listener)
            .add_observer(unlink_listeners)
            .add_observer(listener_unlinked)
            .add_systems(
                PostUpdate,
                sync_server_readiness.before(NetworkTopologySystems::Update),
            );
    }
}
/// Start a plaintext WebSocket listener sharing this app's logical authority.
pub fn listen(world: &mut World, address: SocketAddr) -> Entity {
    let socket = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(65536))
        .max_frame_size(Some(65536))
        .max_write_buffer_size(262144);
    let config = ServerConfig::builder()
        .with_bind_address(address)
        .with_no_encryption()
        .with_socket_config(socket);
    listen_websocket(world, config)
}
/// Start a WebSocket listener with explicit TLS/proxy configuration.
pub fn listen_websocket(world: &mut World, config: ServerConfig) -> Entity {
    let address = config.bind_address();
    let server = authority_entity(world);
    let entity = world
        .spawn((
            Listener { server },
            ChildOf(server),
            LocalAddr(address),
            WebSocketServerIo { config },
        ))
        .id();
    world.trigger(Start { entity });
    entity
}
/// Add QUIC/TLS to the same App/authority as the WebSocket listener.
pub fn listen_webtransport(
    world: &mut World,
    address: SocketAddr,
    certificate: WebTransportIdentity,
) -> Entity {
    let server = authority_entity(world);
    let entity = world
        .spawn((
            Listener { server },
            ChildOf(server),
            LocalAddr(address),
            WebTransportServerIo { certificate },
        ))
        .id();
    world.trigger(Start { entity });
    entity
}

fn authority_entity(world: &mut World) -> Entity {
    if let Some(entity) = world
        .query_filtered::<Entity, With<TransportAuthority>>()
        .iter(world)
        .next()
    {
        return entity;
    }
    world.spawn((TransportAuthority, RawServer)).id()
}

// Lightyear 0.30 supports only one Started server and one PeerId::Server mapping.
// Its IO wrappers each own a separate Aeronet endpoint (a one-to-one relation),
// so keep them as Link listeners and aggregate their readiness on the authority.
fn sync_server_readiness(
    listeners: Query<(&Listener, Has<Linked>)>,
    servers: Query<(Entity, Has<Linked>), With<TransportAuthority>>,
    mut commands: Commands,
) {
    for (server, linked) in &servers {
        let ready = listeners
            .iter()
            .any(|(listener, ready)| listener.server == server && ready);
        if ready && !linked {
            commands.entity(server).insert(Linked);
        } else if !ready && linked {
            commands.entity(server).insert(Unlinked {
                reason: UnlinkReason::ServerStopped,
            });
        }
    }
}

fn stop_listener(event: On<Stop>, listeners: Query<(), With<Listener>>, mut commands: Commands) {
    if listeners.contains(event.entity) {
        commands.trigger(Unlink {
            entity: event.entity,
            reason: UnlinkReason::ServerStopped,
        });
    }
}

fn unlink_listeners(
    event: On<Unlink>,
    servers: Query<(), With<TransportAuthority>>,
    listeners: Query<(Entity, &Listener)>,
    mut commands: Commands,
) {
    if servers.contains(event.entity) {
        for (entity, listener) in &listeners {
            if listener.server == event.entity {
                commands.trigger(Unlink {
                    entity,
                    reason: event.reason.clone(),
                });
            }
        }
    }
}

fn listener_unlinked(
    event: On<Add, Unlinked>,
    listeners: Query<(&Unlinked, &Children), With<Listener>>,
    mut commands: Commands,
) {
    if let Ok((unlinked, peers)) = listeners.get(event.entity) {
        for peer in peers.iter() {
            commands.trigger(Unlink {
                entity: peer,
                reason: unlinked.reason.clone(),
            });
            commands.entity(peer).try_despawn();
        }
    }
}

fn new_peer(
    event: On<Add, LinkOf>,
    links: Query<&LinkOf>,
    listeners: Query<&Listener>,
    mut commands: Commands,
) {
    let Ok(link) = links.get(event.entity) else {
        return;
    };
    let Ok(listener) = listeners.get(link.server) else {
        return;
    };
    commands.entity(event.entity).insert((
        TransportPeer,
        // Ownership stays with the endpoint for transport teardown; Lightyear's
        // connection identity and routing point to the common RawServer.
        ChildOf(link.server),
        LinkOf {
            server: listener.server,
        },
    ));
}
