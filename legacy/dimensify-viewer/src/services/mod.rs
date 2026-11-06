use bevy::prelude::*;

#[cfg(feature = "transport")]
pub mod protocol_response;
#[cfg(feature = "transport")]
pub mod transport_bridge;

#[allow(unused)]
pub fn plugin(app: &mut App) {
    #[cfg(feature = "transport")]
    {
        app.add_plugins(transport_bridge::plugin);
        app.add_plugins(protocol_response::plugin);
        app.add_plugins(dimensify_transport::TransportRuntimePlugin::default());
    }
}
