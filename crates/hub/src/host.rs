use crate::http;
use bevy::log::LogPlugin;
use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use dimensify_session::{
    SharedDocument,
    server::{
        CollaborationListener, CollaborationServerPlugin, WebTransportIdentity, listen,
        listen_websocket, listen_webtransport,
    },
};
use lightyear::prelude::{Linked, LocalAddr, Unlinked};
use std::{net::SocketAddr, time::Duration};

/// Listener and ingress configuration independent of application schema or accounts.
#[derive(Clone, Debug)]
pub struct HostConfig {
    pub address: SocketAddr,
    pub certificate: Option<String>,
    pub key: Option<String>,
    pub quic_address: Option<SocketAddr>,
    pub http: bool,
    pub plaintext_ws: bool,
    pub endpoint_description: &'static str,
}
impl Default for HostConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:5002".parse().unwrap(),
            certificate: None,
            key: None,
            quic_address: None,
            http: false,
            plaintext_ws: false,
            endpoint_description: "Dimensify collaboration WebSocket endpoint. Connect using a client; readiness: /health\n",
        }
    }
}
impl HostConfig {
    /// Read bind/TLS/HTTP settings using an application-selected environment prefix.
    pub fn from_env(prefix: &str, default_address: SocketAddr) -> Result<Self, String> {
        Self::from_lookup(prefix, default_address, |key| std::env::var(key).ok())
    }
    fn from_lookup(
        prefix: &str,
        default_address: SocketAddr,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, String> {
        let value = |suffix| lookup(&format!("{prefix}_{suffix}"));
        let address = |suffix| {
            value(suffix)
                .map(|v| {
                    v.parse::<SocketAddr>()
                        .map_err(|_| format!("{prefix}_{suffix} must be a socket address"))
                })
                .transpose()
        };
        let flag = |suffix| match value(suffix).as_deref() {
            None | Some("0") => Ok(false),
            Some("1") => Ok(true),
            _ => Err(format!("{prefix}_{suffix} must be 0 or 1")),
        };
        let result = Self {
            address: address("BIND")?.unwrap_or(default_address),
            certificate: value("TLS_CERT"),
            key: value("TLS_KEY"),
            quic_address: address("QUIC_BIND")?,
            http: flag("HTTP")?,
            plaintext_ws: flag("WS_PLAINTEXT")?,
            ..default()
        };
        result.validate()?;
        Ok(result)
    }
    /// Reject partial TLS configuration and incompatible HTTP/QUIC listeners before binding.
    pub fn validate(&self) -> Result<(), String> {
        if self.certificate.is_some() != self.key.is_some()
            || (self.quic_address.is_some() && self.certificate.is_none())
        {
            return Err("WebTransport requires both a TLS certificate and private key".into());
        }
        if self.http
            && (self.certificate.is_some() || self.key.is_some() || self.quic_address.is_some())
        {
            return Err("HTTP ingress requires proxy-terminated TLS, without WebTransport".into());
        }
        Ok(())
    }
}

/// Run a headless authority using the caller's document schema and listener settings.
pub fn run<D: SharedDocument>(config: HostConfig) -> Result<AppExit, Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16))))
        .add_plugins(LogPlugin::default());
    configure::<D>(&mut app, config)?;
    Ok(app.run())
}

/// Install session hosting in an app with MinimalPlugins, without initializing graphics.
pub fn configure<D: SharedDocument>(
    app: &mut App,
    config: HostConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    config.validate()?;
    app.add_plugins(CollaborationServerPlugin::<D>::default())
        .add_systems(Update, report_listener);
    let address = if config.http {
        app.insert_resource(http::HttpIngress::start(
            config.address,
            config.endpoint_description,
        )?)
        .add_systems(Update, http::heartbeat);
        "127.0.0.1:0".parse().unwrap()
    } else {
        config.address
    };
    match (config.certificate, config.key) {
        (Some(certificate), Some(key)) => {
            let quic_address = config.quic_address.unwrap_or(address);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let identity =
                runtime.block_on(WebTransportIdentity::load_pemfiles(certificate, key))?;
            if config.plaintext_ws {
                listen(app.world_mut(), address);
            } else {
                let socket = lightyear::websocket::server::ServerConfig::builder()
                    .with_bind_address(address).with_tls_config(websocket_tls(&identity))
                    .with_socket_config(lightyear::websocket::prelude::tungstenite::protocol::WebSocketConfig::default()
                        .max_message_size(Some(65536)).max_frame_size(Some(65536)).max_write_buffer_size(262144));
                listen_websocket(app.world_mut(), socket);
            }
            listen_webtransport(app.world_mut(), quic_address, identity);
        }
        (None, None) => {
            listen(app.world_mut(), address);
        }
        _ => unreachable!("validated TLS pair"),
    }
    Ok(())
}

/// Reuse the QUIC certificate for TCP TLS; both transports share one WSS address.
fn websocket_tls(
    identity: &WebTransportIdentity,
) -> lightyear::websocket::prelude::rustls::ServerConfig {
    use lightyear::websocket::prelude::rustls::{self, pki_types::*};
    rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("TLS protocol versions")
    .with_no_client_auth()
    .with_single_cert(
        identity
            .certificate_chain()
            .as_slice()
            .iter()
            .map(|certificate| CertificateDer::from(certificate.der().to_vec()))
            .collect(),
        PrivatePkcs8KeyDer::from(identity.private_key().secret_der().to_vec()).into(),
    )
    .expect("Could not configure WebSocket TLS")
}

#[allow(
    clippy::type_complexity,
    reason = "The listener query spells out its readiness filter and optional QUIC transport"
)]
fn report_listener(
    started: Query<
        (
            &LocalAddr,
            Option<&lightyear::prelude::server::WebTransportServerIo>,
        ),
        (With<CollaborationListener>, Added<Linked>),
    >,
    failed: Query<(), (With<CollaborationListener>, Added<Unlinked>)>,
    mut exit: MessageWriter<AppExit>,
) {
    for (address, quic) in &started {
        let transport = if quic.is_some() {
            "WebTransport"
        } else {
            "WebSocket"
        };
        info!("Collaboration {transport} listening on {}", address.0);
    }
    if !failed.is_empty() {
        error!("Collaboration listener failed or stopped");
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn websocket_tls_handshake_uses_the_quic_certificate() {
        use lightyear::websocket::prelude::rustls::{self, pki_types::*};
        use std::{io::Cursor, sync::Arc};
        let identity = WebTransportIdentity::self_signed(["localhost"]).unwrap();
        let leaf = CertificateDer::from(identity.certificate_chain().as_slice()[0].der().to_vec());
        let mut roots = rustls::RootCertStore::empty();
        roots.add(leaf.clone()).unwrap();
        let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let mut client = rustls::ClientConnection::new(
            Arc::new(client_config),
            ServerName::try_from("localhost").unwrap(),
        )
        .unwrap();
        let mut server = rustls::ServerConnection::new(Arc::new(websocket_tls(&identity))).unwrap();
        // Exercise a validated TLS handshake in memory, without opening sandboxed sockets.
        for _ in 0..10 {
            let mut bytes = Vec::new();
            client.write_tls(&mut bytes).unwrap();
            server.read_tls(&mut Cursor::new(bytes)).unwrap();
            server.process_new_packets().unwrap();
            let mut bytes = Vec::new();
            server.write_tls(&mut bytes).unwrap();
            client.read_tls(&mut Cursor::new(bytes)).unwrap();
            client.process_new_packets().unwrap();
            if !client.is_handshaking() && !server.is_handshaking() {
                break;
            }
        }
        assert!(!client.is_handshaking());
        assert!(!server.is_handshaking());
        assert_eq!(client.peer_certificates().unwrap()[0], leaf);
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    #[test]
    fn caller_selects_prefix_and_default_without_product_configuration() {
        let c =
            HostConfig::from_lookup("TEST", "127.0.0.1:5010".parse().unwrap(), |key| match key {
                "TEST_HTTP" => Some("1".into()),
                _ => None,
            })
            .unwrap();
        assert!(c.http);
        assert_eq!(c.address.port(), 5010);
        assert!(c.certificate.is_none());
    }
    #[test]
    fn invalid_flags_and_partial_or_conflicting_tls_fail_before_binding() {
        assert!(
            HostConfig::from_lookup("TEST", "127.0.0.1:0".parse().unwrap(), |_| Some(
                "invalid".into()
            ))
            .is_err()
        );
        let mut c = HostConfig {
            key: Some("private-key.pem".into()),
            ..Default::default()
        };
        assert!(c.validate().is_err());
        c.certificate = Some("certificate.pem".into());
        assert!(c.validate().is_ok());
        c.http = true;
        assert!(c.validate().is_err());
    }
}
