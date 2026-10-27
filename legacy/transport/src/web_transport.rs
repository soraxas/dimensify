use bevy_app::prelude::*;
use bevy_ecs::{error::Result, prelude::*};
use bevy_internal::{
    prelude::MinimalPlugins,
    time::{Time, Timer, TimerMode},
};
use bevy_log::{LogPlugin, info};
use dimensify_protocol::{ProtoRequest, ProtoResponse, TransportError};
use lightyear::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Mutex,
        mpsc::{Receiver, Sender},
    },
    time::Duration,
};

#[cfg(all(feature = "webtransport", not(target_family = "wasm")))]
use lightyear::prelude::Identity;
#[cfg(feature = "udp")]
use lightyear::prelude::UdpIo;
#[cfg(feature = "webtransport")]
use lightyear::prelude::client::WebTransportClientIo;
#[cfg(feature = "udp")]
use lightyear::prelude::server::ServerUdpIo;
#[cfg(all(feature = "webtransport", not(target_family = "wasm")))]
use lightyear::prelude::server::WebTransportServerIo;
use lightyear::prelude::{
    Connect,
    client::{ClientPlugins, RawClient},
    server::{RawServer, ServerPlugins, Start},
};
#[cfg(feature = "udp")]
use lightyear_udp::UdpPlugin;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StreamBytes {
    pub payload: Vec<u8>,
}

pub struct StreamReliable;

pub struct StreamUnreliable;

pub fn register_messages(app: &mut App) {
    app.register_message::<StreamBytes>();
    app.register_message::<ProtoRequest>();
    app.register_message::<ProtoResponse>();

    app.add_channel::<StreamReliable>(ChannelSettings {
        mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
        ..Default::default()
    })
    .add_direction(NetworkDirection::Bidirectional);

    app.add_channel::<StreamUnreliable>(ChannelSettings {
        mode: ChannelMode::UnorderedUnreliable,
        ..Default::default()
    })
    .add_direction(NetworkDirection::Bidirectional);
}

pub struct TransportPlugin;

impl Plugin for TransportPlugin {
    fn build(&self, app: &mut App) {
        register_messages(app);
    }
}

pub struct TransportRuntimePlugin {
    pub config: crate::TransportConfig,
}

impl Default for TransportRuntimePlugin {
    fn default() -> Self {
        Self {
            config: crate::TransportConfig::from_env(),
        }
    }
}

impl Plugin for TransportRuntimePlugin {
    fn build(&self, app: &mut App) {
        register_messages(app);
        app.insert_resource(self.config.clone());

        match self.config.connection {
            crate::TransportConnection::Server => {
                app.add_plugins(ServerPlugins {
                    tick_duration: Duration::from_secs_f32(1.0 / self.config.tick_hz),
                });
                app.add_observer(insert_message_components_for_linkof);
                app.add_systems(Update, ensure_message_components_for_linkof);
            }
            crate::TransportConnection::Client => {
                app.add_plugins(ClientPlugins {
                    tick_duration: Duration::from_secs_f32(1.0 / self.config.tick_hz),
                });
                if matches!(self.config.mode, crate::TransportMode::Udp)
                    && !app.is_plugin_added::<UdpPlugin>()
                {
                    app.add_plugins(UdpPlugin);
                }
            }
        }

        app.add_systems(Startup, setup_transport_endpoint);
        if transport_debug_enabled() {
            app.init_resource::<TransportDebugTimer>();
            app.add_systems(Update, debug_transport_state);
        }
    }
}

pub struct TransportController {
    request_tx: Sender<ProtoRequest>,
    response_rx: Receiver<ProtoResponse>,
    _handle: std::thread::JoinHandle<()>,
}

impl TransportController {
    pub fn start(config: crate::TransportConfig) -> Self {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let (response_tx, response_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            if transport_debug_enabled() {
                app.add_plugins(LogPlugin::default());
            }
            app.add_plugins(TransportRuntimePlugin { config });

            app.insert_resource(TransportQueue {
                request_rx: Mutex::new(request_rx),
                response_tx,
                pending: Vec::new(),
            });

            app.add_systems(Update, send_requests);
            app.add_systems(Update, collect_responses);
            // Ensure plugin finish hooks run (MessagePlugin/TransportPlugin build systems here).
            app.finish();
            app.cleanup();

            loop {
                app.update();
                std::thread::sleep(Duration::from_millis(16));
            }
        });

        Self {
            request_tx,
            response_rx,
            _handle: handle,
        }
    }

    pub fn send(&self, request: ProtoRequest) -> Result<(), String> {
        self.request_tx.send(request).map_err(|err| err.to_string())
    }

    pub fn send_and_wait(
        &self,
        request: ProtoRequest,
        timeout: Duration,
    ) -> Result<ProtoResponse, String> {
        self.send(request)?;
        self.response_rx
            .recv_timeout(timeout)
            .map_err(|err| err.to_string())
    }

    pub fn try_recv(&self) -> Option<ProtoResponse> {
        self.response_rx.try_recv().ok()
    }
}

#[derive(Resource)]
struct TransportQueue {
    request_rx: Mutex<Receiver<ProtoRequest>>,
    response_tx: Sender<ProtoResponse>,
    pending: Vec<ProtoRequest>,
}

fn send_requests(
    mut queue: ResMut<TransportQueue>,
    mut senders: Query<&mut MessageSender<ProtoRequest>, With<Connected>>,
) {
    let mut drained = Vec::new();
    if let Ok(rx) = queue.request_rx.lock() {
        while let Ok(request) = rx.try_recv() {
            drained.push(request);
        }
    }
    if !drained.is_empty() {
        queue.pending.extend(drained);
    }

    if queue.pending.is_empty() {
        return;
    }

    let mut sender = match senders.iter_mut().next() {
        Some(sender) => sender,
        None => {
            if transport_debug_enabled() {
                bevy_log::info!("transport: no connected sender yet");
            }
            return;
        }
    };

    for request in queue.pending.drain(..) {
        sender.send::<StreamReliable>(request);
    }
}

fn collect_responses(
    queue: Res<TransportQueue>,
    mut receivers: Query<&mut MessageReceiver<ProtoResponse>, With<Connected>>,
) {
    for mut receiver in &mut receivers {
        for response in receiver.receive() {
            let _ = queue.response_tx.send(response);
        }
    }
}

fn setup_transport_endpoint(
    mut commands: Commands,
    config: Res<crate::TransportConfig>,
) -> Result<(), BevyError> {
    let entity = match config.connection {
        crate::TransportConnection::Server => spawn_server(&mut commands, config.as_ref())?,
        crate::TransportConnection::Client => spawn_client(&mut commands, config.as_ref()),
    };
    commands.trigger(Connect { entity });
    Ok(())
}

#[derive(Resource)]
struct TransportDebugTimer {
    timer: Timer,
}

impl Default for TransportDebugTimer {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(1.0, TimerMode::Repeating),
        }
    }
}

fn transport_debug_enabled() -> bool {
    std::env::var("DIMENSIFY_TRANSPORT_DEBUG")
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn debug_transport_state(
    time: Res<Time>,
    mut timer: ResMut<TransportDebugTimer>,
    config: Res<crate::TransportConfig>,
    connected: Query<Entity, With<Connected>>,
    linked: Query<Entity, With<Linked>>,
    send_req: Query<Entity, With<MessageSender<ProtoRequest>>>,
    recv_req: Query<Entity, With<MessageReceiver<ProtoRequest>>>,
    send_resp: Query<Entity, With<MessageSender<ProtoResponse>>>,
    recv_resp: Query<Entity, With<MessageReceiver<ProtoResponse>>>,
) {
    if !timer.timer.tick(time.delta()).just_finished() {
        return;
    }
    info!(
        "transport state mode={:?} connection={:?} endpoint={:?} connected={} linked={} send_req={} recv_req={} send_resp={} recv_resp={}",
        config.mode,
        config.connection,
        config.endpoint,
        connected.iter().count(),
        linked.iter().count(),
        send_req.iter().count(),
        recv_req.iter().count(),
        send_resp.iter().count(),
        recv_resp.iter().count(),
    );
}

#[cfg(not(target_family = "wasm"))]
fn spawn_server(
    commands: &mut Commands,
    config: &crate::TransportConfig,
) -> Result<Entity, TransportError> {
    let mut entity = commands.spawn((
        Name::from("TransportServer"),
        Server::default(),
        Link::new(None),
        LocalAddr(config.server_addr),
        RawServer,
    ));

    match config.mode {
        #[cfg(feature = "webtransport")]
        crate::TransportMode::WebTransport => {
            let certificate = load_certificate(config);
            entity.insert(WebTransportServerIo { certificate });
        }
        #[cfg(feature = "websocket")]
        crate::TransportMode::WebSocket => {
            use lightyear::prelude::server::WebSocketServerIo;
            entity.insert(WebSocketServerIo::default());
        }
        #[cfg(feature = "udp")]
        crate::TransportMode::Udp => {
            entity.insert(ServerUdpIo::default());
        }
    }

    Ok(entity.id())
}

#[cfg(target_family = "wasm")]
fn spawn_server(_commands: &mut Commands, _config: &crate::TransportConfig) -> Entity {
    panic!("transport server is not supported on wasm");
}

fn spawn_client(commands: &mut Commands, config: &crate::TransportConfig) -> Entity {
    let mut entity = commands.spawn((
        Name::from("TransportClient"),
        Client::default(),
        Link::new(None),
        PeerAddr(config.server_addr),
        RawClient,
    ));

    match config.mode {
        #[cfg(feature = "webtransport")]
        crate::TransportMode::WebTransport => {
            use lightyear::prelude::client::WebTransportClientIo;
            entity.insert(WebTransportClientIo {
                certificate_digest: config.certificate_digest.clone(),
            });
        }
        #[cfg(feature = "websocket")]
        crate::TransportMode::WebSocket => {
            use lightyear::prelude::client::WebSocketClientIo;
            entity.insert(WebSocketClientIo::default());
        }
        #[cfg(feature = "udp")]
        crate::TransportMode::Udp => {
            let local_addr = config
                .client_addr
                .unwrap_or_else(|| "0.0.0.0:0".parse().expect("valid fallback address"));
            entity.insert(LocalAddr(local_addr));
            entity.insert(UdpIo::default());
        }
    }

    let endpoint = config.endpoint.clone();
    insert_message_components(&mut entity, &endpoint);
    entity.id()
}

fn insert_message_components_for_linkof(
    trigger: On<Add, LinkOf>,
    config: Res<crate::TransportConfig>,
    mut commands: Commands,
) {
    let mut entity = commands.entity(trigger.entity);
    insert_message_components(&mut entity, &config.endpoint);
}

fn insert_message_components(entity: &mut EntityCommands, endpoint: &crate::TransportEndpoint) {
    entity.insert(MessageManager::default());
    match endpoint {
        crate::TransportEndpoint::Viewer => {
            entity.insert(MessageReceiver::<ProtoRequest>::default());
            entity.insert(MessageSender::<ProtoResponse>::default());
        }
        crate::TransportEndpoint::Controller => {
            entity.insert(MessageReceiver::<ProtoResponse>::default());
            entity.insert(MessageSender::<ProtoRequest>::default());
        }
    }
}

fn ensure_message_components_for_linkof(
    config: Res<crate::TransportConfig>,
    mut commands: Commands,
    query: Query<(Entity, &LinkOf), Without<MessageManager>>,
) {
    if !matches!(config.endpoint, crate::TransportEndpoint::Viewer) {
        return;
    }
    for (entity, _link_of) in &query {
        let mut entity = commands.entity(entity);
        insert_message_components(&mut entity, &config.endpoint);
    }
}

#[cfg(all(feature = "webtransport", not(target_family = "wasm")))]
fn load_certificate(config: &crate::TransportConfig) -> Identity {
    if let (Some(cert_path), Some(key_path)) =
        (&config.certificate_path, &config.certificate_key_path)
    {
        return pollster::block_on(Identity::load_pemfiles(cert_path, key_path))
            .expect("failed to load transport certificate");
    }

    Identity::self_signed(["localhost", "127.0.0.1"]).expect("failed to generate certificate")
}

#[cfg(any(not(feature = "webtransport"), target_family = "wasm"))]
fn load_certificate(_config: &crate::TransportConfig) -> ! {
    panic!("transport server certificates are not supported on wasm");
}
