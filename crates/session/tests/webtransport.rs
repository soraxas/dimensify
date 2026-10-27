#![cfg(all(feature = "client", feature = "server", not(target_family = "wasm")))]
use bevy::prelude::*;
use dimensify_session::{server::*, *};
use lightyear::prelude::{
    Connected, Link, LinkOf, Linked, LocalAddr, NetworkTopology, NetworkingMetadata, PeerAddr,
    PeerId, UnlinkReason, Unlinked,
};
use std::time::{Duration, Instant};
#[path = "../examples/common/mod.rs"]
mod common;
use aeronet_websocket::rustls;
use aeronet_webtransport::wtransport;
use common::Cube;

fn tick_until(
    server: &mut App,
    clients: &mut [&mut App],
    mut done: impl FnMut(&[&mut App]) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(14);
    loop {
        server.update();
        for app in clients.iter_mut() {
            app.update();
        }
        if done(clients) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "transport did not reach expected state"
        );
        std::thread::sleep(Duration::from_millis(3));
    }
}

fn fixture(quic: bool) -> (App, String, TransportClientConfig, Option<Entity>) {
    let identity = WebTransportIdentity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    let leaf = &identity.certificate_chain().as_slice()[0];
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls::pki_types::CertificateDer::from(leaf.der().to_vec()))
        .unwrap();
    let hash = leaf.hash();
    let client_config = TransportClientConfig {
        webtransport: std::sync::Arc::new(move || {
            wtransport::ClientConfig::builder()
                .with_bind_default()
                .with_server_certificate_hashes([hash.clone()])
                .build()
        }),
        websocket: lightyear::websocket::prelude::client::ClientConfig::builder().with_tls_config(
            rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth(),
        ),
    };
    let ws_identity = lightyear::websocket::server::Identity::new(
        identity
            .certificate_chain()
            .as_slice()
            .iter()
            .map(|c| rustls::pki_types::CertificateDer::from(c.der().to_vec())),
        rustls::pki_types::PrivatePkcs8KeyDer::from(identity.private_key().secret_der().to_vec())
            .into(),
    );
    let mut server = App::new();
    server
        .add_plugins(MinimalPlugins)
        .add_plugins(CollaborationServerPlugin::<Cube>::default());
    let listener = listen_websocket(
        server.world_mut(),
        lightyear::websocket::server::ServerConfig::builder()
            .with_bind_address("127.0.0.1:0".parse().unwrap())
            .with_identity(ws_identity),
    );
    server.finish();
    server.cleanup();
    tick_until(&mut server, &mut [], |_| true);
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.world().get::<Linked>(listener).is_none() {
        server.update();
        std::thread::sleep(Duration::from_millis(3));
        assert!(Instant::now() < deadline);
    }
    let address = server.world().get::<LocalAddr>(listener).unwrap().0;
    let wt = quic.then(|| listen_webtransport(server.world_mut(), address, identity));
    (
        server,
        format!("wss://{address}/collab/"),
        client_config,
        wt,
    )
}
fn client(config: TransportClientConfig) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .add_plugins(CollaborationClientPlugin::<Cube>::default());
    app.finish();
    app.cleanup();
    app
}
fn joined(app: &App) -> bool {
    app.world().resource::<ClientSession<Cube>>().status == ConnectionStatus::Joined
}
fn create(app: &mut App, endpoint: &str) {
    connect::<Cube>(app.world_mut(), endpoint).unwrap();
    app.world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .send(ClientRequest::Create {
            name: "host".into(),
        })
        .unwrap();
}

#[test]
fn listener_readiness_forms_one_lightyear_server() {
    #[derive(Resource, Default)]
    struct UnlinkRequests(Vec<Entity>);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(CollaborationServerPlugin::<Cube>::default())
        .init_resource::<UnlinkRequests>()
        .add_observer(
            |event: On<lightyear::prelude::Unlink>, mut requests: ResMut<UnlinkRequests>| {
                requests.0.push(event.entity);
            },
        );
    app.finish();
    app.cleanup();
    let ws = listen(app.world_mut(), "127.0.0.1:0".parse().unwrap());
    let wt = listen_webtransport(
        app.world_mut(),
        "127.0.0.1:0".parse().unwrap(),
        WebTransportIdentity::self_signed(["localhost"]).unwrap(),
    );
    // Inject endpoint readiness without polling socket tasks in PreUpdate.
    // This checks the same Lightyear lifecycle that actual IO drives.
    app.world_mut()
        .entity_mut(ws)
        .insert(lightyear::prelude::Linked);
    app.world_mut()
        .entity_mut(wt)
        .insert(lightyear::prelude::Linked);
    app.world_mut().run_schedule(PostUpdate);
    assert!(
        app.world()
            .resource::<lightyear::prelude::NetworkingMetadata>()
            .mode
            .is_headless_server(),
        "invalid topology: {:?}",
        app.world()
            .resource::<lightyear::prelude::NetworkingMetadata>()
            .mode
    );
    let NetworkTopology::Server(server) = app.world().resource::<NetworkingMetadata>().mode else {
        unreachable!();
    };
    assert_eq!(
        app.world()
            .resource::<NetworkingMetadata>()
            .peer_map
            .get(&PeerId::Server),
        Some(&server)
    );
    let mut peers = Vec::new();
    for (i, listener) in [ws, wt].into_iter().enumerate() {
        let peer = app
            .world_mut()
            .spawn((
                LinkOf { server: listener },
                Link::default(),
                PeerAddr(format!("127.0.0.1:{}", 6000 + i).parse().unwrap()),
            ))
            .id();
        app.world_mut().entity_mut(peer).insert(Linked);
        assert_eq!(app.world().get::<LinkOf>(peer).unwrap().server, server);
        assert!(
            app.world().get::<Connected>(peer).is_some(),
            "raw connection must use the shared authority"
        );
        peers.push(peer);
    }
    // Losing one listener only tears down its peers, without changing the
    // server identity or disconnecting the other transport's participants.
    app.world_mut()
        .trigger(lightyear::prelude::server::Stop { entity: wt });
    app.world_mut().flush();
    let requests = &app.world().resource::<UnlinkRequests>().0;
    assert!(requests.contains(&wt));
    assert!(!requests.contains(&ws) && !requests.contains(&peers[0]));
    app.world_mut().entity_mut(wt).insert(Unlinked {
        reason: UnlinkReason::ServerStopped,
    });
    app.world_mut().run_schedule(PostUpdate);
    assert!(app.world().get_entity(peers[1]).is_err());
    assert!(app.world().get::<Connected>(peers[0]).is_some());
    assert_eq!(
        app.world().resource::<NetworkingMetadata>().mode,
        NetworkTopology::Server(server)
    );
    assert_eq!(
        app.world()
            .resource::<NetworkingMetadata>()
            .peer_map
            .get(&PeerId::Server),
        Some(&server)
    );

    app.world_mut().entity_mut(ws).insert(Unlinked {
        reason: UnlinkReason::ServerStopped,
    });
    app.world_mut().run_schedule(PostUpdate);
    assert!(app.world().get_entity(peers[0]).is_err());
    assert_eq!(
        app.world().resource::<NetworkingMetadata>().mode,
        NetworkTopology::Undefined
    );
    assert!(
        !app.world()
            .resource::<NetworkingMetadata>()
            .peer_map
            .contains_key(&PeerId::Server)
    );

    // Restarting an endpoint reuses the same authority.
    app.world_mut().entity_mut(wt).insert(Linked);
    app.world_mut().run_schedule(PostUpdate);
    assert_eq!(
        app.world().resource::<NetworkingMetadata>().mode,
        NetworkTopology::Server(server)
    );

    // Stopping the authority fans out to every endpoint.
    app.world_mut().resource_mut::<UnlinkRequests>().0.clear();
    app.world_mut()
        .trigger(lightyear::prelude::server::Stop { entity: server });
    app.world_mut().flush();
    let requests = &app.world().resource::<UnlinkRequests>().0;
    assert!(requests.contains(&ws) && requests.contains(&wt));
}

#[test]
fn quic_and_websocket_participants_share_one_authority() {
    let (mut server, endpoint, config, _) = fixture(true);
    let mut host = client(config.clone());
    create(&mut host, &endpoint);
    tick_until(&mut server, &mut [&mut host], |apps| joined(apps[0]));
    let session = host.world().resource::<ClientSession<Cube>>();
    assert_eq!(session.transport_kind(), Some(TransportKind::WebTransport));
    let room = session.snapshot.as_ref().unwrap().room.clone();
    let invitation = session.invitations.as_ref().unwrap().viewer.clone();
    let plain = listen(server.world_mut(), "127.0.0.1:0".parse().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.world().get::<Linked>(plain).is_none() {
        server.update();
        std::thread::sleep(Duration::from_millis(3));
        assert!(Instant::now() < deadline);
    }
    let plain_endpoint = format!("ws://{}", server.world().get::<LocalAddr>(plain).unwrap().0);
    let mut viewer = client(config);
    connect::<Cube>(viewer.world_mut(), &plain_endpoint).unwrap();
    viewer
        .world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .send(ClientRequest::Join {
            room: room.clone(),
            invitation,
            name: "viewer".into(),
        })
        .unwrap();
    tick_until(&mut server, &mut [&mut host, &mut viewer], |apps| {
        joined(apps[1])
            && apps[0]
                .world()
                .resource::<ClientSession<Cube>>()
                .snapshot
                .as_ref()
                .unwrap()
                .participants
                .len()
                == 2
    });
    let v = viewer.world().resource::<ClientSession<Cube>>();
    assert_eq!(v.transport_kind(), Some(TransportKind::WebSocket));
    assert_eq!(v.snapshot.as_ref().unwrap().room, room);
    assert!(
        server
            .world()
            .resource::<lightyear::prelude::NetworkingMetadata>()
            .mode
            .is_headless_server(),
        "both transports must belong to one valid Lightyear server: {:?}",
        server
            .world()
            .resource::<lightyear::prelude::NetworkingMetadata>()
            .mode
    );
}

#[test]
fn blocked_quic_falls_back_once_and_keeps_queued_create() {
    let (mut server, endpoint, config, _) = fixture(false);
    // Bind but never service UDP: emulate a black hole instead of an immediate refusal.
    let url = url::Url::parse(&endpoint).unwrap();
    let _blackhole = std::net::UdpSocket::bind(("127.0.0.1", url.port().unwrap())).unwrap();
    let mut host = client(config);
    create(&mut host, &endpoint);
    tick_until(&mut server, &mut [&mut host], |apps| joined(apps[0]));
    let session = host.world().resource::<ClientSession<Cube>>();
    assert_eq!(session.transport_kind(), Some(TransportKind::WebSocket));
    assert_eq!(session.snapshot.as_ref().unwrap().participants.len(), 1);
    assert!(session.invitations.is_some());
    assert_eq!(
        host.world_mut()
            .query_filtered::<Entity, With<lightyear::prelude::client::RawClient>>()
            .iter(host.world())
            .count(),
        1
    );
}

#[test]
fn reconnect_can_resume_same_identity_over_fallback() {
    let (mut server, endpoint, config, wt) = fixture(true);
    let mut host = client(config);
    create(&mut host, &endpoint);
    tick_until(&mut server, &mut [&mut host], |apps| joined(apps[0]));
    let original = host.world().resource::<ClientSession<Cube>>().you;
    let room = host
        .world()
        .resource::<ClientSession<Cube>>()
        .snapshot
        .as_ref()
        .unwrap()
        .room
        .clone();
    // A new connection replaces the old transport but retains its endpoint-bound credential.
    let wt = wt.unwrap();
    server
        .world_mut()
        .trigger(lightyear::prelude::server::Stop { entity: wt });
    connect::<Cube>(host.world_mut(), &endpoint).unwrap();
    tick_until(&mut server, &mut [&mut host], |apps| joined(apps[0]));
    let session = host.world().resource::<ClientSession<Cube>>();
    assert_eq!(session.transport_kind(), Some(TransportKind::WebSocket));
    assert_eq!(session.you, original);
    let snapshot = session.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.room, room);
    assert_eq!(Some(snapshot.owner), original);
    assert_eq!(snapshot.participants.len(), 1);
}

#[test]
fn endpoint_probe_uses_fallback_without_joining() {
    let (mut server, endpoint, config, _) = fixture(false);
    let mut app = client(config);
    probe_endpoint::<Cube>(app.world_mut(), &endpoint).unwrap();
    tick_until(&mut server, &mut [&mut app], |apps| {
        !apps[0].world().resource::<EndpointProbe<Cube>>().checking()
    });
    assert_eq!(
        app.world_mut()
            .resource_mut::<EndpointProbe<Cube>>()
            .take_result()
            .unwrap()
            .result,
        Ok(())
    );
    assert_eq!(
        app.world().resource::<ClientSession<Cube>>().status,
        ConnectionStatus::Disconnected
    );
    assert!(
        app.world()
            .resource::<ClientSession<Cube>>()
            .snapshot
            .is_none()
    );
    assert_eq!(
        app.world_mut()
            .query_filtered::<Entity, With<lightyear::prelude::client::RawClient>>()
            .iter(app.world())
            .count(),
        0
    );
}

#[test]
fn cancelled_quic_releases_its_udp_socket_and_capacity() {
    let blackhole = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let reserved = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let local = reserved.local_addr().unwrap();
    let socket = std::sync::Mutex::new(Some(reserved));
    let config = TransportClientConfig {
        webtransport: std::sync::Arc::new(move || {
            wtransport::ClientConfig::builder()
                .with_bind_socket(socket.lock().unwrap().take().unwrap())
                .with_native_certs()
                .max_idle_timeout(Some(Duration::from_millis(250)))
                .unwrap()
                .build()
        }),
        ..Default::default()
    };
    let mut app = client(config);
    let endpoint = format!("wss://{}", blackhole.local_addr().unwrap());
    connect::<Cube>(app.world_mut(), &endpoint).unwrap();
    assert!(
        std::net::UdpSocket::bind(local).is_err(),
        "QUIC owns the configured UDP socket"
    );
    disconnect::<Cube>(app.world_mut());
    // Frontend cancellation is immediate, but Quinn releases its socket
    // asynchronously after connection-close draining. Check the real socket,
    // not just entity disappearance (which no longer waits for a timeout).
    let deadline = Instant::now() + Duration::from_secs(6);
    let _rebound = loop {
        app.update();
        let retained = app
            .world_mut()
            .query_filtered::<Entity, With<aeronet_webtransport::client::WebTransportClient>>()
            .iter(app.world())
            .count();
        if retained == 0
            && let Ok(socket) = std::net::UdpSocket::bind(local)
        {
            break socket;
        }
        assert!(
            Instant::now() < deadline,
            "cancelled handshake must release its frontend and UDP socket"
        );
        std::thread::sleep(Duration::from_millis(3));
    };
    assert_eq!(
        app.world().resource::<ClientSession<Cube>>().status,
        ConnectionStatus::Disconnected
    );
}

#[test]
fn cancelled_quic_late_success_is_closed_without_joining() {
    let (mut server, endpoint, config, _) = fixture(true);
    let mut app = client(config);
    create(&mut app, &endpoint);
    disconnect::<Cube>(app.world_mut());
    tick_until(&mut server, &mut [&mut app], |apps| {
        let world = apps[0].world();
        !world
            .iter_entities()
            .any(|e| e.contains::<aeronet_webtransport::client::WebTransportClient>())
    });
    assert_eq!(
        app.world().resource::<ClientSession<Cube>>().status,
        ConnectionStatus::Disconnected
    );
    assert!(
        app.world()
            .resource::<ClientSession<Cube>>()
            .snapshot
            .is_none()
    );
}
