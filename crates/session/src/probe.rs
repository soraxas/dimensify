//! A temporary protocol round trip, isolated from room membership and credentials.
use crate::{
    transport::{Attempt, AttemptStatus, Endpoint, close},
    *,
};
use bevy::prelude::*;
use std::marker::PhantomData;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeResult {
    pub generation: u64,
    pub endpoint: String,
    /// Transport that completed the check (or the final failed attempt).
    pub transport: TransportKind,
    pub failures: Vec<TransportFailure>,
    pub result: Result<(), SessionError>,
}
struct Active {
    generation: u64,
    endpoint: String,
    attempt: Attempt,
}
#[derive(Resource)]
pub struct EndpointProbe<D: SharedDocument> {
    generation: u64,
    active: Option<Active>,
    result: Option<ProbeResult>,
    marker: PhantomData<D>,
}
impl<D: SharedDocument> Default for EndpointProbe<D> {
    fn default() -> Self {
        Self {
            generation: 0,
            active: None,
            result: None,
            marker: PhantomData,
        }
    }
}
impl<D: SharedDocument> EndpointProbe<D> {
    /// Report whether a compatibility probe is currently in flight.
    pub fn checking(&self) -> bool {
        self.active.is_some()
    }
    /// Consume the completed probe result, if available.
    pub fn take_result(&mut self) -> Option<ProbeResult> {
        self.result.take()
    }
    /// Return the endpoint under inspection.
    pub fn endpoint(&self) -> Option<&str> {
        self.active.as_ref().map(|p| p.endpoint.as_str())
    }
    /// Transport currently being checked; changes when the probe falls back.
    pub fn transport_kind(&self) -> Option<TransportKind> {
        self.active.as_ref().map(|p| p.attempt.kind)
    }
    /// Return transport diagnostics collected by this probe.
    pub fn transport_failures(&self) -> &[TransportFailure] {
        self.active.as_ref().map_or(&[], |p| &p.attempt.failures)
    }
}
/// Cancel the active probe and retire its transport.
pub fn cancel_probe<D: SharedDocument>(world: &mut World) {
    let active = world.resource_mut::<EndpointProbe<D>>().active.take();
    if let Some(active) = active {
        close(world, active.attempt.entity);
    }
    let mut probe = world.resource_mut::<EndpointProbe<D>>();
    probe.generation = probe.generation.saturating_add(1);
    probe.result = None;
}
/// Start a protocol compatibility probe without joining a room.
pub fn probe_endpoint<D: SharedDocument>(
    world: &mut World,
    endpoint: &str,
) -> Result<u64, SessionError> {
    let endpoint = Endpoint::parse(endpoint)?;
    cancel_probe::<D>(world);
    let attempt = Attempt::start::<crate::transport::SessionHandshake<D>>(world, endpoint.clone());
    let mut probe = world.resource_mut::<EndpointProbe<D>>();
    let generation = probe.generation;
    probe.active = Some(Active {
        generation,
        endpoint: endpoint.websocket,
        attempt,
    });
    Ok(generation)
}
/// Advance the active probe and publish its completion result.
pub(crate) fn pump<D: SharedDocument>(world: &mut World) {
    let Some(mut active) = world.resource_mut::<EndpointProbe<D>>().active.take() else {
        return;
    };
    let outcome = match active
        .attempt
        .poll::<crate::transport::SessionHandshake<D>>(world)
    {
        AttemptStatus::Pending => None,
        AttemptStatus::Ready => Some(Ok(())),
        AttemptStatus::Failed => Some(Err(SessionError::Transport)),
    };
    if let Some(result) = outcome {
        close(world, active.attempt.entity);
        world.resource_mut::<EndpointProbe<D>>().result = Some(ProbeResult {
            generation: active.generation,
            endpoint: active.endpoint,
            transport: active.attempt.kind,
            failures: active.attempt.failures,
            result,
        });
    } else {
        world.resource_mut::<EndpointProbe<D>>().active = Some(active);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::tests::TestDocument;
    use std::time::Duration;
    #[test]
    fn failed_probe_retains_both_attempt_reasons_after_cleanup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CollaborationClientPlugin::<TestDocument>::default());
        app.finish();
        app.cleanup();
        probe_endpoint::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        let entity = app
            .world()
            .resource::<EndpointProbe<TestDocument>>()
            .active
            .as_ref()
            .unwrap()
            .attempt
            .entity;
        app.world_mut()
            .entity_mut(entity)
            .insert(lightyear::prelude::Unlinked {
                reason: lightyear::prelude::UnlinkReason::TransportError(
                    "certificate rejected".into(),
                ),
            });
        pump::<TestDocument>(app.world_mut());
        assert_eq!(
            app.world()
                .resource::<EndpointProbe<TestDocument>>()
                .transport_kind(),
            Some(TransportKind::WebSocket)
        );
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(8));
        pump::<TestDocument>(app.world_mut());
        let result = app
            .world_mut()
            .resource_mut::<EndpointProbe<TestDocument>>()
            .take_result()
            .unwrap();
        assert!(result.result.is_err());
        // The completed check must retain useful diagnostics after its entities are removed.
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures[0].transport, TransportKind::WebTransport);
        assert!(result.failures[0].reason.contains("certificate rejected"));
        assert_eq!(result.failures[1].transport, TransportKind::WebSocket);
        assert!(result.failures[1].reason.contains("timed out"));
    }
    #[test]
    fn timeout_and_cancel_remove_entities_and_late_results() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CollaborationClientPlugin::<TestDocument>::default());
        app.finish();
        app.cleanup();
        probe_endpoint::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        let first = app
            .world()
            .resource::<EndpointProbe<TestDocument>>()
            .active
            .as_ref()
            .unwrap()
            .attempt
            .entity;
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(3));
        pump::<TestDocument>(app.world_mut());
        assert!(app.world().get_entity(first).is_err());
        let fallback = app
            .world()
            .resource::<EndpointProbe<TestDocument>>()
            .active
            .as_ref()
            .unwrap()
            .attempt
            .entity;
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(8));
        pump::<TestDocument>(app.world_mut());
        assert!(app.world().get_entity(fallback).is_err());
        assert!(
            app.world_mut()
                .resource_mut::<EndpointProbe<TestDocument>>()
                .take_result()
                .unwrap()
                .result
                .is_err()
        );
        probe_endpoint::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        let cancelled = app
            .world()
            .resource::<EndpointProbe<TestDocument>>()
            .active
            .as_ref()
            .unwrap()
            .attempt
            .entity;
        cancel_probe::<TestDocument>(app.world_mut());
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(20));
        pump::<TestDocument>(app.world_mut());
        assert!(app.world().get_entity(cancelled).is_err());
        assert!(
            !app.world()
                .resource::<EndpointProbe<TestDocument>>()
                .checking()
        );
        assert!(
            app.world_mut()
                .resource_mut::<EndpointProbe<TestDocument>>()
                .take_result()
                .is_none()
        );
    }
}
