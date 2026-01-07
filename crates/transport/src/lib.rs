use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportMode {
    WebTransport,
    WebSocket,
    Udp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub mode: TransportMode,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            mode: TransportMode::WebTransport,
        }
    }
}

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
mod lightyear_support {
    use bevy::prelude::*;
    use lightyear::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
    pub struct StreamBytes {
        pub payload: Vec<u8>,
    }

    pub struct StreamReliable;

    pub struct StreamUnreliable;

    pub fn register_messages(app: &mut App) {
        app.register_message::<StreamBytes>();

        app.add_channel::<StreamReliable>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        });

        app.add_channel::<StreamUnreliable>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        });
    }

    pub struct TransportPlugin;

    impl Plugin for TransportPlugin {
        fn build(&self, app: &mut App) {
            register_messages(app);
        }
    }
}

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
pub use lightyear_support::{StreamBytes, StreamReliable, StreamUnreliable, TransportPlugin};

#[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
pub fn register_messages(app: &mut bevy::prelude::App) {
    lightyear_support::register_messages(app);
}

#[cfg(not(any(feature = "webtransport", feature = "websocket", feature = "udp")))]
pub fn register_messages(_app: &mut bevy::prelude::App) {}

#[cfg(test)]
mod tests {
    use super::TransportConfig;
    use bevy::prelude::App;

    #[test]
    fn default_transport_config() {
        let config = TransportConfig::default();
        assert!(matches!(config.mode, super::TransportMode::WebTransport));
    }

    #[test]
    #[cfg(any(feature = "webtransport", feature = "websocket", feature = "udp"))]
    fn register_messages_with_lightyear() {
        let mut app = App::new();
        super::register_messages(&mut app);
    }
}
