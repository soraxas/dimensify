//! Transient pointer presence, independent of scene documents and edit authority.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How the sender chose a world-space pointing location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerKind {
    Surface,
    FocusPlane,
}

/// Canonical repository coordinates (+Z up), picked using the sender's rendered view.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldPointer {
    pub position: [f32; 3],
    pub kind: PointerKind,
}

/// Transient participant view, independent of presenter/edit authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpatialPresence {
    pub camera: CameraPose,
    /// None hides only the pointer; the participant camera can remain visible.
    pub pointer: Option<WorldPointer>,
}
impl SpatialPresence {
    /// Validate finite, bounded coordinates and a normalized camera orientation.
    pub fn valid(&self) -> bool {
        valid_pose(&self.camera)
            && self
                .pointer
                .is_none_or(|p| p.position.iter().all(|v| v.is_finite() && v.abs() <= 1e7))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorSubscription {
    #[serde(deserialize_with = "crate::wire::bounded_text")]
    pub room: RoomId,
    /// Monotonic within a socket; separates disable/re-enable cycles.
    pub generation: u64,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CursorSample {
    #[serde(deserialize_with = "crate::wire::bounded_text")]
    pub room: RoomId,
    pub epoch: u64,
    pub scene_revision: u64,
    pub generation: u64,
    pub sequence: u64,
    /// Offset from viewport center in units of viewport height, Y down.
    /// None hides the pointer (outside viewport, unfocused, or disabled).
    pub position: Option<[f32; 2]>,
    /// Optional spatial view in the room's shared scene frame and revision.
    pub spatial: Option<SpatialPresence>,
}
impl CursorSample {
    /// Match the authoritative room and shared-scene context before display/fanout.
    pub fn matches<D>(&self, room: &RoomSnapshot<D>) -> bool {
        self.room == room.room
            && self.epoch == room.camera_epoch
            && self.scene_revision == room.document.revision
    }
    /// Validate both legacy screen cursors and spatial participant views.
    pub fn valid(&self) -> bool {
        self.generation > 0
            && self.sequence > 0
            && self.position.is_none_or(|[x, y]| {
                x.is_finite() && y.is_finite() && x.abs() <= 16. && y.abs() <= 0.5
            })
            && self.spatial.as_ref().is_none_or(SpatialPresence::valid)
    }
    fn pointer_visible(&self) -> bool {
        self.position.is_some() || self.spatial.as_ref().is_some_and(|s| s.pointer.is_some())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteCursor {
    pub participant: ParticipantId,
    /// Recipient's subscription, so in-flight packets cannot undo opt-out.
    pub subscription: u64,
    pub room_version: u64,
    pub sample: CursorSample,
}

#[cfg(feature = "client")]
#[derive(Default, PartialEq, Eq)]
pub(crate) enum SubscriptionDelivery {
    #[default]
    Pending,
    AwaitingAck,
    Confirmed,
}
#[cfg(feature = "client")]
#[derive(Default)]
pub(crate) struct CursorClient {
    pub subscription: Option<CursorSubscription>,
    pub outgoing: Option<CursorSample>,
    pub remote: BTreeMap<ParticipantId, RemoteCursor>,
    // Local semantic stream identity survives coalesced hide/show arrivals.
    pub streams: BTreeMap<ParticipantId, u64>,
    next_stream: u64,
    pub sequence: u64,
    pub generation: u64,
    pub delivery: SubscriptionDelivery,
}
#[cfg(feature = "client")]
impl CursorClient {
    /// Clear subscriptions, pending samples and remote presence for the retired room.
    pub fn reset_room(&mut self) {
        // Leave/rejoin on the same socket must never reuse subscription tokens.
        *self = Self {
            generation: self.generation,
            ..Default::default()
        };
    }
    /// Accept a remote cursor only for the current subscription and room context.
    pub fn receive<D>(&mut self, cursor: RemoteCursor, room: &RoomSnapshot<D>, you: Option<u64>) {
        let subscribed = self.subscription.as_ref().is_some_and(|s| {
            s.enabled && s.room == cursor.sample.room && s.generation == cursor.subscription
        });
        if !subscribed
            || Some(cursor.participant) == you
            || !cursor.sample.valid()
            || cursor.sample.room != room.room
            || cursor.sample.epoch < room.camera_epoch
            || cursor.sample.scene_revision < room.document.revision
            || (cursor.room_version <= room.version
                && !room.participants.iter().any(|p| p.id == cursor.participant))
            || (!self.remote.contains_key(&cursor.participant)
                && self.remote.len() >= MAX_PARTICIPANTS)
        {
            return;
        }
        let order = (cursor.sample.generation, cursor.sample.sequence);
        if self
            .remote
            .get(&cursor.participant)
            .is_some_and(|old| order <= (old.sample.generation, old.sample.sequence))
        {
            return;
        }
        if (!cursor.sample.pointer_visible()
            && self
                .remote
                .get(&cursor.participant)
                .is_some_and(|old| old.sample.pointer_visible()))
            || self
                .remote
                .get(&cursor.participant)
                .is_none_or(|old| old.sample.generation != cursor.sample.generation)
        {
            self.next_stream = self.next_stream.saturating_add(1);
            self.streams.insert(cursor.participant, self.next_stream);
        }
        self.remote.insert(cursor.participant, cursor);
    }
    /// Discard presence retired by the latest room snapshot.
    pub fn reconcile<D>(&mut self, room: &RoomSnapshot<D>) {
        self.remote.retain(|id, c| {
            c.sample.room == room.room
                && c.sample.epoch >= room.camera_epoch
                && c.sample.scene_revision >= room.document.revision
                && (c.room_version > room.version || room.participants.iter().any(|p| p.id == *id))
        });
        self.streams.retain(|id, _| self.remote.contains_key(id));
    }
}

#[cfg(feature = "server")]
#[derive(Default)]
pub(crate) struct CursorRoom(BTreeMap<ParticipantId, CursorPeer>);
#[cfg(feature = "server")]
struct CursorPeer {
    subscription: CursorSubscription,
    latest: Option<CursorSample>,
}
#[cfg(feature = "server")]
impl CursorRoom {
    /// Forget a participant and its cursor subscription.
    pub fn remove(&mut self, peer: ParticipantId) {
        self.0.remove(&peer);
    }
    /// Check that the recipient still accepts this subscription generation.
    pub fn accepts(&self, peer: ParticipantId, generation: u64) -> bool {
        self.0
            .get(&peer)
            .is_some_and(|p| p.subscription.enabled && p.subscription.generation == generation)
    }
    /// Update cursor opt-in and emit acknowledgement and retirement messages.
    pub fn subscribe<D>(
        &mut self,
        peer: ParticipantId,
        next: CursorSubscription,
        room: &RoomSnapshot<D>,
    ) -> Result<Vec<(u64, ServerMessage<D>)>, SessionError> {
        if next.room != room.room || next.generation == 0 {
            return Err(SessionError::Invalid);
        }
        if self
            .0
            .get(&peer)
            .is_some_and(|p| next.generation <= p.subscription.generation)
        {
            return Err(SessionError::Stale);
        }
        let ack = ServerMessage::CursorSubscription(next.clone());
        let previous = self.0.insert(
            peer,
            CursorPeer {
                subscription: next,
                latest: None,
            },
        );
        let mut messages = vec![(peer, ack)];
        if let Some(mut sample) = previous.and_then(|p| p.latest) {
            sample.position = None;
            sample.spatial = None;
            sample.sequence = u64::MAX;
            messages.extend(self.broadcast(peer, sample, room.version));
        }
        Ok(messages)
    }
    /// Validate a presence sample and route it to subscribed participants.
    pub fn publish<D>(
        &mut self,
        peer: ParticipantId,
        sample: CursorSample,
        room: &RoomSnapshot<D>,
    ) -> Result<Vec<(u64, ServerMessage<D>)>, SessionError> {
        if !sample.valid()
            || !sample.matches(room)
            || (sample.spatial.is_none() && sample.position.is_some() && room.presenter.is_none())
        {
            return Err(SessionError::Invalid);
        }
        let publisher = self.0.get_mut(&peer).ok_or(SessionError::Unauthorized)?;
        if !publisher.subscription.enabled || publisher.subscription.generation != sample.generation
        {
            return Err(SessionError::Unauthorized);
        }
        if publisher
            .latest
            .as_ref()
            .is_some_and(|old| sample.sequence <= old.sequence)
        {
            return Err(SessionError::Stale);
        }
        publisher.latest = Some(sample.clone());
        Ok(self.broadcast(peer, sample, room.version))
    }
    fn broadcast<D>(
        &self,
        from: ParticipantId,
        sample: CursorSample,
        room_version: u64,
    ) -> Vec<(u64, ServerMessage<D>)> {
        self.0
            .iter()
            .filter(|(id, p)| **id != from && p.subscription.enabled)
            .map(|(id, p)| {
                (
                    *id,
                    ServerMessage::Cursor(RemoteCursor {
                        participant: from,
                        subscription: p.subscription.generation,
                        room_version,
                        sample: sample.clone(),
                    }),
                )
            })
            .collect()
    }
}
