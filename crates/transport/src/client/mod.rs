//! Connection selection shared by real sessions and endpoint probes.
//! A logical WSS endpoint maps to HTTPS/QUIC first, then the original WSS URL.
use bevy::{ecs::system::EntityCommand, prelude::*};
use lightyear::prelude::{client::*, *};
use std::{collections::HashSet, sync::Arc, time::Duration};

/// Invalid endpoint syntax or unsupported/unsafe URL fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidEndpoint;
impl std::fmt::Display for InvalidEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("expected ws:// or wss:// endpoint without credentials or fragment")
    }
}
impl std::error::Error for InvalidEndpoint {}

/// Application handshake invoked before a connection becomes usable.
pub trait ConnectionProtocol {
    /// Install the application's message components on a fresh transport entity.
    fn initialize(world: &mut World, entity: Entity);
    /// Advance the handshake, setting `sent` for reliable-ACK accounting.
    fn poll(world: &mut World, entity: Entity, sent: &mut bool) -> bool;
}

/// Install message transport while leaving application simulation clocks unchanged.
#[derive(Default)]
pub struct TransportClientPlugin;
impl Plugin for TransportClientPlugin {
    fn build(&self, app: &mut App) {
        let tick_duration = app
            .world()
            .get_resource::<Time<Fixed>>()
            .map_or_else(|| Time::<Fixed>::default().timestep(), Time::timestep);
        let clients = ClientPlugins { tick_duration }
            .build()
            .disable::<lightyear_sync::client::ClientPlugin>();
        #[cfg(target_family = "wasm")]
        let clients = clients.disable::<lightyear_web::WebKeepalivePlugin>();
        app.add_plugins((lightyear_sync::plugin::TimelineSyncPlugin, clients))
            .init_resource::<TransportClientConfig>()
            .init_resource::<WebTransportFailures>();
    }
}

const QUIC_TIMEOUT: Duration = Duration::from_secs(3);
const WEBSOCKET_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportKind {
    WebTransport,
    WebSocket,
}

/// Local diagnostic only; never sent as part of the room protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportFailure {
    pub transport: TransportKind,
    pub reason: String,
}

/// TLS options for native embeddings and browser certificate-hash development.
/// Defaults validate certificates using the platform's normal trust store.
#[derive(Resource, Clone)]
pub struct TransportClientConfig {
    /// Create a fresh config per attempt (native configs can own a UDP socket).
    pub webtransport: Arc<dyn Fn() -> aeronet_webtransport::client::ClientConfig + Send + Sync>,
    pub websocket: lightyear::websocket::prelude::client::ClientConfig,
}
impl Default for TransportClientConfig {
    fn default() -> Self {
        // Both transports enable Rustls providers. Match Aeronet WebSocket's
        // default even when an embedding builds this resource before plugins.
        #[cfg(not(target_family = "wasm"))]
        let _ = lightyear::websocket::prelude::rustls::crypto::aws_lc_rs::default_provider()
            .install_default();
        Self {
            webtransport: Arc::new(|| {
                #[cfg(not(target_family = "wasm"))]
                {
                    aeronet_webtransport::client::ClientConfig::builder()
                        .with_bind_default()
                        .with_native_certs()
                        .keep_alive_interval(Some(Duration::from_secs(1)))
                        .max_idle_timeout(Some(Duration::from_secs(5)))
                        .expect("valid QUIC idle timeout")
                        .build()
                }
                #[cfg(target_family = "wasm")]
                {
                    aeronet_webtransport::client::ClientConfig {
                        congestion_control:
                            aeronet_webtransport::xwt_web::CongestionControl::LowLatency,
                        ..default()
                    }
                }
            }),
            websocket: default(),
        }
    }
}

#[derive(Clone)]
pub struct Endpoint {
    pub websocket: String,
    webtransport: Option<String>,
}
impl Endpoint {
    /// Validate an endpoint and derive its supported transport addresses.
    pub fn parse(endpoint: &str) -> Result<Self, InvalidEndpoint> {
        let mut url = url::Url::parse(endpoint).map_err(|_| InvalidEndpoint)?;
        if endpoint.len() > 2048
            || !matches!(url.scheme(), "ws" | "wss")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(InvalidEndpoint);
        }
        let websocket = url.to_string();
        let webtransport = if url.scheme() == "wss" {
            url.set_scheme("https").map_err(|_| InvalidEndpoint)?;
            Some(url.to_string())
        } else {
            None
        };
        Ok(Self {
            websocket,
            webtransport,
        })
    }
}

fn webtransport_available() -> bool {
    #[cfg(target_family = "wasm")]
    {
        js_sys::Reflect::get(&js_sys::global(), &"WebTransport".into())
            .is_ok_and(|constructor| constructor.is_function())
    }
    #[cfg(not(target_family = "wasm"))]
    {
        true
    }
}

#[derive(Component)]
pub struct ProtocolReady;

/// Endpoint-scoped fallback preference; automatic recovery must not probe QUIC again.
#[derive(Resource, Default)]
pub struct WebTransportFailures(HashSet<String>);

/// Allow a fresh WebTransport attempt after an explicit user-requested reconnect.
/// Do not call from automatic recovery or background endpoint checks.
pub fn retry_webtransport_on_next_connect(
    world: &mut World,
    endpoint: &str,
) -> Result<(), InvalidEndpoint> {
    let endpoint = Endpoint::parse(endpoint)?;
    world
        .resource_mut::<WebTransportFailures>()
        .0
        .remove(&endpoint.websocket);
    Ok(())
}

pub enum AttemptStatus {
    Pending,
    Ready,
    Failed,
}

pub struct Attempt {
    pub entity: Entity,
    pub kind: TransportKind,
    endpoint: Endpoint,
    started: Duration,
    pub sent: bool,
    ready: bool,
    pub failures: Vec<TransportFailure>,
}
impl Attempt {
    /// Start the preferred transport using the caller's handshake protocol.
    pub fn start<P: ConnectionProtocol>(world: &mut World, endpoint: Endpoint) -> Self {
        let kind = if endpoint.webtransport.is_some()
            && webtransport_available()
            && !world
                .resource::<WebTransportFailures>()
                .0
                .contains(&endpoint.websocket)
        {
            TransportKind::WebTransport
        } else {
            TransportKind::WebSocket
        };
        Self::spawn::<P>(world, endpoint, kind)
    }
    fn spawn<P: ConnectionProtocol>(
        world: &mut World,
        endpoint: Endpoint,
        kind: TransportKind,
    ) -> Self {
        let config = world.resource::<TransportClientConfig>().clone();
        let entity = world
            .spawn((
                RawClient,
                // Browser transports do not expose a local socket address to RawClient.
                LocalAddr(std::net::SocketAddr::from(([0, 0, 0, 0], 0))),
            ))
            .id();
        P::initialize(world, entity);
        match kind {
            TransportKind::WebSocket => {
                world.entity_mut(entity).insert(WebSocketClientIo::from_url(
                    config.websocket,
                    &endpoint.websocket,
                ));
                world.trigger(Connect { entity });
            }
            TransportKind::WebTransport => {
                // Use the same Aeronet/Lightyear bridge, with configurable validated TLS.
                // Lightyear 0.30.1's native wrapper otherwise uses an empty hash allowlist.
                world.trigger(Connect { entity });
                let target = endpoint
                    .webtransport
                    .clone()
                    .expect("QUIC requires a secure endpoint");
                #[cfg(not(target_family = "wasm"))]
                let target = {
                    use aeronet_webtransport::wtransport::endpoint::IntoConnectOptions;
                    target.into_options()
                };
                let peer = world.spawn((lightyear_aeronet::AeronetLinkOf(entity),));
                aeronet_webtransport::client::WebTransportClient::connect(
                    (config.webtransport)(),
                    target,
                )
                .apply(peer);
            }
        }
        Self {
            entity,
            kind,
            endpoint,
            started: world.resource::<Time<Real>>().elapsed(),
            sent: false,
            ready: false,
            failures: Vec::new(),
        }
    }
    /// Advance handshake or fallback and return the current attempt status.
    pub fn poll<P: ConnectionProtocol>(&mut self, world: &mut World) -> AttemptStatus {
        if self.ready {
            return AttemptStatus::Ready;
        }
        let timeout = match self.kind {
            TransportKind::WebTransport => QUIC_TIMEOUT,
            TransportKind::WebSocket => WEBSOCKET_TIMEOUT,
        };
        if world.get_entity(self.entity).is_err()
            || world.get::<Unlinked>(self.entity).is_some()
            || world
                .resource::<Time<Real>>()
                .elapsed()
                .saturating_sub(self.started)
                >= timeout
        {
            let reason = if let Some(unlinked) = world.get::<Unlinked>(self.entity) {
                unlinked.reason.to_string()
            } else if world.get_entity(self.entity).is_err() {
                "Connection closed before the server check completed".into()
            } else if self.sent {
                "Server protocol response timed out".into()
            } else {
                "Connection timed out before the server could be verified".into()
            };
            self.failures.push(TransportFailure {
                transport: self.kind,
                reason: reason
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .take(512)
                    .collect(),
            });
            close(world, self.entity);
            if self.kind == TransportKind::WebTransport {
                world
                    .resource_mut::<WebTransportFailures>()
                    .0
                    .insert(self.endpoint.websocket.clone());
                let failures = std::mem::take(&mut self.failures);
                *self = Self::spawn::<P>(world, self.endpoint.clone(), TransportKind::WebSocket);
                self.failures = failures;
                return AttemptStatus::Pending;
            }
            return AttemptStatus::Failed;
        }
        if world.get::<Connected>(self.entity).is_some() {
            self.ready = P::poll(world, self.entity, &mut self.sent);
            if self.ready {
                world.entity_mut(self.entity).insert(ProtocolReady);
                return AttemptStatus::Ready;
            }
        }
        AttemptStatus::Pending
    }
}
/// Disconnect and retire the selected transport entity.
pub fn close(world: &mut World, entity: Entity) {
    // The patched Aeronet handshake watches frontend cancellation. Despawning
    // the linked frontend drops its receiver and cancels pending backend work.
    if world.get_entity(entity).is_ok() {
        world.trigger(Disconnect { entity });
        world.despawn(entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secure_endpoint_preserves_service_identity_and_never_downgrades_tls() {
        let endpoint = Endpoint::parse("wss://EXAMPLE.com:443/collab/?tenant=one").unwrap();
        assert_eq!(endpoint.websocket, "wss://example.com/collab/?tenant=one");
        assert_eq!(
            endpoint.webtransport.as_deref(),
            Some("https://example.com/collab/?tenant=one")
        );
        let ipv6 = Endpoint::parse("wss://[::1]:4433/collab/").unwrap();
        assert_eq!(
            ipv6.webtransport.as_deref(),
            Some("https://[::1]:4433/collab/")
        );
        assert!(
            Endpoint::parse("ws://localhost:5000/")
                .unwrap()
                .webtransport
                .is_none()
        );
        for invalid in [
            "https://example.com",
            "wss://user:secret@example.com",
            "wss://example.com/#credential",
        ] {
            assert!(Endpoint::parse(invalid).is_err());
        }
    }
}
