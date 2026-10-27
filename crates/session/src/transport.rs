//! Room handshake adapter for the transport's protocol-independent attempts.
use crate::{protocol::*, *};
use bevy::prelude::*;
pub(super) use dimensify_transport::client::{
    Attempt, AttemptStatus, Endpoint, ProtocolReady, close,
};
pub use dimensify_transport::{TransportClientConfig, TransportFailure, TransportKind};
use lightyear::prelude::*;

impl From<dimensify_transport::client::InvalidEndpoint> for SessionError {
    fn from(_: dimensify_transport::client::InvalidEndpoint) -> Self {
        Self::Invalid
    }
}

/// Reset endpoint fallback after an explicit user reconnect request.
pub fn retry_webtransport_on_next_connect(
    world: &mut World,
    endpoint: &str,
) -> Result<(), SessionError> {
    dimensify_transport::client::retry_webtransport_on_next_connect(world, endpoint)
        .map_err(Into::into)
}

pub(super) struct SessionHandshake<D>(std::marker::PhantomData<D>);
impl<D: SharedDocument> dimensify_transport::client::ConnectionProtocol for SessionHandshake<D> {
    fn initialize(world: &mut World, entity: Entity) {
        world.entity_mut(entity).insert((
            MessageSender::<ClientEnvelope<D>>::default(),
            MessageReceiver::<ServerEnvelope<D>>::default(),
        ));
    }
    fn poll(world: &mut World, entity: Entity, sent: &mut bool) -> bool {
        if !*sent
            && let Some(mut sender) = world.get_mut::<MessageSender<ClientEnvelope<D>>>(entity)
        {
            sender.send::<ControlChannel>(Envelope::new(ClientRequest::Probe { nonce: 0 }));
            *sent = true;
        }
        world
            .get_mut::<MessageReceiver<ServerEnvelope<D>>>(entity)
            .is_some_and(|mut receiver| {
                receiver
                    .receive()
                    .any(|msg| matches!(msg.message, ServerMessage::Probe { nonce: 0 }))
            })
    }
}
