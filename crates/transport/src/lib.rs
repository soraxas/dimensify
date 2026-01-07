use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportMode {
    WebTransport,
    WebSocket,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportConnection {
    Server,
    Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportEndpoint {
    Viewer,
    Controller,
}

#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct TransportConfig {
    pub mode: TransportMode,
    pub connection: TransportConnection,
    pub endpoint: TransportEndpoint,
    pub server_addr: SocketAddr,
    pub client_addr: Option<SocketAddr>,
    pub certificate_digest: String,
    pub certificate_path: Option<String>,
    pub certificate_key_path: Option<String>,
    pub tick_hz: f32,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            mode: TransportMode::WebTransport,
            connection: TransportConnection::Server,
            endpoint: TransportEndpoint::Viewer,
            server_addr: "127.0.0.1:6210".parse().expect("valid default address"),
            client_addr: None,
            certificate_digest: String::new(),
            certificate_path: None,
            certificate_key_path: None,
            tick_hz: 60.0,
        }
    }
}

impl TransportConfig {
    #[cfg(not(target_family = "wasm"))]
    pub fn from_env() -> Self {
        use std::env;

        let mut config = Self::default();

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_MODE") {
            config.mode = parse_mode(&value).unwrap_or(config.mode);
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_CONNECTION") {
            config.connection = parse_connection(&value).unwrap_or(config.connection);
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_ENDPOINT") {
            config.endpoint = parse_endpoint(&value).unwrap_or(config.endpoint);
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_SERVER_ADDR") {
            if let Ok(addr) = value.parse() {
                config.server_addr = addr;
            }
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_CLIENT_ADDR") {
            if let Ok(addr) = value.parse() {
                config.client_addr = Some(addr);
            }
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_CERT_DIGEST") {
            config.certificate_digest = value;
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_CERT_PATH") {
            config.certificate_path = Some(value);
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_CERT_KEY_PATH") {
            config.certificate_key_path = Some(value);
        }

        if let Ok(value) = env::var("DIMENSIFY_TRANSPORT_TICK_HZ") {
            if let Ok(hz) = value.parse() {
                config.tick_hz = hz;
            }
        }

        config
    }

    #[cfg(target_family = "wasm")]
    pub fn from_env() -> Self {
        let mut config = Self::default();
        config.connection = TransportConnection::Client;
        config
    }
}

fn parse_mode(input: &str) -> Option<TransportMode> {
    match input.to_ascii_lowercase().as_str() {
        "webtransport" => Some(TransportMode::WebTransport),
        "websocket" => Some(TransportMode::WebSocket),
        "udp" => Some(TransportMode::Udp),
        _ => None,
    }
}

fn parse_connection(input: &str) -> Option<TransportConnection> {
    match input.to_ascii_lowercase().as_str() {
        "server" => Some(TransportConnection::Server),
        "client" => Some(TransportConnection::Client),
        _ => None,
    }
}

fn parse_endpoint(input: &str) -> Option<TransportEndpoint> {
    match input.to_ascii_lowercase().as_str() {
        "viewer" => Some(TransportEndpoint::Viewer),
        "controller" => Some(TransportEndpoint::Controller),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerRequest {
    ApplyJson { payload: String },
    Remove { name: String },
    List,
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerResponse {
    Ack,
    Entities { names: Vec<String> },
    Error { message: String },
}

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
mod lightyear_support {
    use bevy::log::LogPlugin;
    use bevy::prelude::*;
    use lightyear::prelude::*;
    use serde::{Deserialize, Serialize};
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, Sender};
    use std::time::Duration;

    use lightyear::prelude::Connect;
    #[cfg(all(feature = "webtransport", not(target_family = "wasm")))]
    use lightyear::prelude::Identity;
    #[cfg(feature = "udp")]
    use lightyear::prelude::UdpIo;
    #[cfg(feature = "websocket")]
    use lightyear::prelude::client::WebSocketClientIo;
    #[cfg(feature = "webtransport")]
    use lightyear::prelude::client::WebTransportClientIo;
    use lightyear::prelude::client::{ClientPlugins, RawClient};
    #[cfg(feature = "udp")]
    use lightyear::prelude::server::ServerUdpIo;
    #[cfg(all(feature = "websocket", not(target_family = "wasm")))]
    use lightyear::prelude::server::WebSocketServerIo;
    #[cfg(all(feature = "webtransport", not(target_family = "wasm")))]
    use lightyear::prelude::server::WebTransportServerIo;
    use lightyear::prelude::server::{RawServer, ServerPlugins, Start};
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
        app.register_message::<crate::ViewerRequest>();
        app.register_message::<crate::ViewerResponse>();

        app.add_channel::<StreamReliable>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<StreamUnreliable>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
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
        request_tx: Sender<crate::ViewerRequest>,
        response_rx: Receiver<crate::ViewerResponse>,
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

        pub fn send(&self, request: crate::ViewerRequest) -> Result<(), String> {
            self.request_tx.send(request).map_err(|err| err.to_string())
        }

        pub fn send_and_wait(
            &self,
            request: crate::ViewerRequest,
            timeout: Duration,
        ) -> Option<crate::ViewerResponse> {
            let _ = self.send(request);
            self.response_rx.recv_timeout(timeout).ok()
        }

        pub fn try_recv(&self) -> Option<crate::ViewerResponse> {
            self.response_rx.try_recv().ok()
        }
    }

    #[derive(Resource)]
    struct TransportQueue {
        request_rx: Mutex<Receiver<crate::ViewerRequest>>,
        response_tx: Sender<crate::ViewerResponse>,
        pending: Vec<crate::ViewerRequest>,
    }

    fn send_requests(
        mut queue: ResMut<TransportQueue>,
        mut senders: Query<&mut MessageSender<crate::ViewerRequest>, With<Connected>>,
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
                    info!("transport: no connected sender yet");
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
        mut receivers: Query<&mut MessageReceiver<crate::ViewerResponse>, With<Connected>>,
    ) {
        for mut receiver in &mut receivers {
            for response in receiver.receive() {
                let _ = queue.response_tx.send(response);
            }
        }
    }

    fn setup_transport_endpoint(mut commands: Commands, config: Res<crate::TransportConfig>) {
        match config.connection {
            crate::TransportConnection::Server => {
                let server_entity = spawn_server(&mut commands, config.as_ref());
                commands.trigger(Start {
                    entity: server_entity,
                });
            }
            crate::TransportConnection::Client => {
                let client_entity = spawn_client(&mut commands, config.as_ref());
                commands.trigger(Connect {
                    entity: client_entity,
                });
            }
        }
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
        send_req: Query<Entity, With<MessageSender<crate::ViewerRequest>>>,
        recv_req: Query<Entity, With<MessageReceiver<crate::ViewerRequest>>>,
        send_resp: Query<Entity, With<MessageSender<crate::ViewerResponse>>>,
        recv_resp: Query<Entity, With<MessageReceiver<crate::ViewerResponse>>>,
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
    fn spawn_server(commands: &mut Commands, config: &crate::TransportConfig) -> Entity {
        let mut entity = commands.spawn((
            Name::from("TransportServer"),
            Server::default(),
            Link::new(None),
            LocalAddr(config.server_addr),
            RawServer,
        ));

        match config.mode {
            crate::TransportMode::WebTransport => insert_webtransport_server(&mut entity, config),
            crate::TransportMode::WebSocket => insert_websocket_server(&mut entity),
            crate::TransportMode::Udp => insert_udp_server(&mut entity),
        }

        entity.id()
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
            crate::TransportMode::WebTransport => insert_webtransport_client(&mut entity, config),
            crate::TransportMode::WebSocket => insert_websocket_client(&mut entity),
            crate::TransportMode::Udp => insert_udp_client(&mut entity, config),
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
                entity.insert(MessageReceiver::<crate::ViewerRequest>::default());
                entity.insert(MessageSender::<crate::ViewerResponse>::default());
            }
            crate::TransportEndpoint::Controller => {
                entity.insert(MessageReceiver::<crate::ViewerResponse>::default());
                entity.insert(MessageSender::<crate::ViewerRequest>::default());
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

    #[cfg(all(feature = "webtransport", not(target_family = "wasm")))]
    fn insert_webtransport_server(entity: &mut EntityCommands, config: &crate::TransportConfig) {
        let certificate = load_certificate(config);
        entity.insert(WebTransportServerIo { certificate });
    }

    #[cfg(any(not(feature = "webtransport"), target_family = "wasm"))]
    fn insert_webtransport_server(_entity: &mut EntityCommands, _config: &crate::TransportConfig) {
        panic!("transport server is not supported on wasm");
    }

    #[cfg(feature = "webtransport")]
    fn insert_webtransport_client(entity: &mut EntityCommands, config: &crate::TransportConfig) {
        entity.insert(WebTransportClientIo {
            certificate_digest: config.certificate_digest.clone(),
        });
    }

    #[cfg(not(feature = "webtransport"))]
    fn insert_webtransport_client(_entity: &mut EntityCommands, _config: &crate::TransportConfig) {
        panic!("webtransport transport requires the webtransport feature");
    }

    #[cfg(all(feature = "websocket", not(target_family = "wasm")))]
    fn insert_websocket_server(entity: &mut EntityCommands) {
        entity.insert(WebSocketServerIo::default());
    }

    #[cfg(any(not(feature = "websocket"), target_family = "wasm"))]
    fn insert_websocket_server(_entity: &mut EntityCommands) {
        panic!("websocket transport requires the websocket feature");
    }

    #[cfg(feature = "websocket")]
    fn insert_websocket_client(entity: &mut EntityCommands) {
        entity.insert(WebSocketClientIo::default());
    }

    #[cfg(not(feature = "websocket"))]
    fn insert_websocket_client(_entity: &mut EntityCommands) {
        panic!("websocket transport requires the websocket feature");
    }

    #[cfg(all(feature = "udp", not(target_family = "wasm")))]
    fn insert_udp_server(entity: &mut EntityCommands) {
        entity.insert(ServerUdpIo::default());
    }

    #[cfg(any(not(feature = "udp"), target_family = "wasm"))]
    fn insert_udp_server(_entity: &mut EntityCommands) {
        panic!("udp transport requires the udp feature");
    }

    #[cfg(all(feature = "udp", not(target_family = "wasm")))]
    fn insert_udp_client(entity: &mut EntityCommands, config: &crate::TransportConfig) {
        let local_addr = config
            .client_addr
            .unwrap_or_else(|| "0.0.0.0:0".parse().expect("valid fallback address"));
        entity.insert(LocalAddr(local_addr));
        entity.insert(UdpIo::default());
    }

    #[cfg(any(not(feature = "udp"), target_family = "wasm"))]
    fn insert_udp_client(_entity: &mut EntityCommands, _config: &crate::TransportConfig) {
        panic!("udp transport requires the udp feature");
    }
}

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
pub use lightyear_support::{
    StreamBytes, StreamReliable, StreamUnreliable, TransportController, TransportPlugin,
    TransportRuntimePlugin,
};

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
pub fn register_messages(app: &mut bevy::prelude::App) {
    lightyear_support::register_messages(app);
}

#[cfg(not(any(feature = "webtransport", feature = "websocket", feature = "udp")))]
pub fn register_messages(_app: &mut bevy::prelude::App) {}

#[cfg(test)]
mod tests {
    use super::{TransportConfig, TransportConnection, TransportEndpoint, TransportMode};
    use bevy::prelude::App;

    #[test]
    fn default_transport_config() {
        let config = TransportConfig::default();
        assert!(matches!(config.mode, TransportMode::WebTransport));
        assert!(matches!(config.connection, TransportConnection::Server));
        assert!(matches!(config.endpoint, TransportEndpoint::Viewer));
    }

    #[test]
    #[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
    fn register_messages_with_lightyear() {
        let mut app = App::new();
        super::register_messages(&mut app);
    }
}
