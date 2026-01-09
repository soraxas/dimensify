use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("webtransport transport requires the webtransport feature")]
    WebTransportFeatureRequired,
    #[error("websocket transport requires the websocket feature")]
    WebSocketFeatureRequired,
    #[error("udp transport requires the udp feature")]
    UdpFeatureRequired,
    #[error("invalid transport connection: {0}")]
    InvalidConnection(String),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
}
