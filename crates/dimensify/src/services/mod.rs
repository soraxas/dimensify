use bevy::prelude::*;

pub mod protocol_response;
#[cfg(any(
    feature = "transport_webtransport",
    feature = "transport_websocket",
    feature = "transport_udp"
))]
pub mod transport_bridge;

pub fn plugin(app: &mut App) {
    app.add_plugins(transport_bridge::plugin);
    app.add_plugins(protocol_response::plugin);
}
