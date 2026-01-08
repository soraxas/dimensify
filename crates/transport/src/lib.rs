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

pub use dimensify_protocol::{SceneRequest, ViewerEntityInfo, ViewerEntityKind, ViewerResponse};

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
mod web_transport;

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
pub use web_transport::{
    StreamBytes, StreamReliable, StreamUnreliable, TransportController, TransportPlugin,
    TransportRuntimePlugin,
};

pub fn register_messages(app: &mut bevy::prelude::App) {
    #[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
    {
        web_transport::register_messages(app);
        return;
    }

    bevy::log::error!("Transport features are disabled; cannot register messages");
}

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
