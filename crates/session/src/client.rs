use crate::{
    cursors::SubscriptionDelivery,
    protocol::*,
    transport::{Attempt, AttemptStatus, Endpoint, ProtocolReady},
    *,
};
use bevy::prelude::*;
use lightyear::prelude::{client::*, *};
use lightyear::transport::channel::registry::ChannelKind;
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;
pub const CLIENT_QUEUE_CAPACITY: usize = 64;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Joined,
}
#[derive(Resource, Default)]
pub struct ClientSession<D: SharedDocument> {
    pub status: ConnectionStatus,
    pub you: Option<ParticipantId>,
    pub snapshot: Option<RoomSnapshot<D>>,
    pub invitations: Option<Invitations>,
    pub error: Option<SessionError>,
    resume_credential: Option<ResumeCredential>,
    resuming: bool,
    last_endpoint: Option<String>,
    pending_document: Option<EditId>,
    next_edit: u64,
    pub document_edit_outcome: Option<DocumentEditOutcome>,
    presenter_events: VecDeque<PresenterEvent>,
    spotlight: Option<(RoomId, ParticipantId, u64)>,
    published_camera: Option<CameraSnapshot>,
    camera_published_at: Duration,
    last_received: Duration,
    last_heartbeat: Duration,
    pending: usize,
    last_ack: Duration,
    entity: Option<Entity>,
    attempt: Option<Attempt>,
    transport_failures: Vec<TransportFailure>,
    started: Duration,
    queue: VecDeque<ClientRequest<D>>,
    camera: Option<CameraSnapshot>,
    remote_camera: Option<CameraSnapshot>,
    joining_room: Option<RoomId>,
    cursors: crate::cursors::CursorClient,
    following: Option<(RoomId, Option<ParticipantId>, u64, Option<ParticipantId>)>,
    published_view: Option<CameraSnapshot>,
    view_published_at: Duration,
    outgoing_view: Option<CameraSnapshot>,
    views: BTreeMap<ParticipantId, CameraSnapshot>,
}
#[derive(Clone, Debug)]
pub enum PresenterEvent {
    Rejected {
        room: RoomId,
        request: PresenterRequest,
        error: SessionError,
    },
    Requested {
        room: RoomId,
        request: PresenterRequest,
    },
    Decision {
        room: RoomId,
        request: PresenterRequest,
        granted: bool,
    },
}
impl<D: SharedDocument> ClientSession<D> {
    /// Selected transport for this connection, including an in-progress attempt.
    pub fn transport_kind(&self) -> Option<TransportKind> {
        self.entity
            .and_then(|_| self.attempt.as_ref().map(|a| a.kind))
    }
    /// Failed attempts retained through fallback and final connection failure.
    pub fn transport_failures(&self) -> &[TransportFailure] {
        &self.transport_failures
    }

    /// Drain presenter notifications received since the last call.
    pub fn take_presenter_events(&mut self) -> impl Iterator<Item = PresenterEvent> + '_ {
        self.presenter_events.drain(..)
    }

    /// Consume a one-shot spotlight only while its room and presenter are still current.
    pub fn take_spotlight(&mut self) -> Option<ParticipantId> {
        let (room, presenter, epoch) = self.spotlight.take()?;
        self.snapshot
            .as_ref()
            .filter(|s| {
                self.status == ConnectionStatus::Joined
                    && s.room == room
                    && s.presenter == Some(presenter)
                    && s.camera_epoch == epoch
            })
            .map(|_| presenter)
    }

    /// Last sample submitted to the latest-value camera lane.
    pub fn published_camera(&self) -> Option<&CameraSnapshot> {
        self.published_camera.as_ref()
    }
    /// Return the edit awaiting an authoritative outcome, if any.
    pub fn pending_document_edit(&self) -> Option<EditId> {
        self.pending_document
    }
    /// Submit a registered projection with its write policy and revision guards.
    pub fn submit_projection<P: DocumentProjection<D>>(
        &mut self,
        payload: P::Payload,
    ) -> Result<Option<EditId>, SessionError> {
        if !P::validate(&payload) {
            return Err(SessionError::Invalid);
        }
        let policy = D::registrations().policy::<P>()?;
        let state = self.snapshot.as_ref().ok_or(SessionError::NotJoined)?;
        let editor = state
            .participants
            .iter()
            .any(|p| Some(p.id) == self.you && p.role == Role::Editor);
        if !policy.permits(editor, self.you.is_some() && state.presenter == self.you) {
            return Err(SessionError::Unauthorized);
        }
        if self.pending_document.is_some() {
            return Err(SessionError::Conflict);
        }
        if P::read(&state.document.data) == payload {
            return Ok(None);
        }
        let expected_revision = state.document.revision;
        let mut data = state.document.data.clone();
        P::replace(&mut data, payload);
        D::registrations().apply(
            &state.document.data,
            &data,
            editor,
            state.presenter == self.you,
        )?;
        let id = EditId(self.next_edit.checked_add(1).ok_or(SessionError::Full)?);
        self.send(ClientRequest::SubmitDocument {
            id,
            expected_revision,
            data,
        })?;
        self.next_edit = id.0;
        self.pending_document = Some(id);
        self.document_edit_outcome = None;
        Ok(Some(id))
    }
    /// Latest-value presentation samples; application adapters own pose conversion.
    pub fn publish_camera_sample(
        &mut self,
        pose: CameraPose,
        now: Duration,
    ) -> Result<bool, SessionError> {
        if !D::registrations().has_camera() || !valid_pose(&pose) {
            return Err(SessionError::Invalid);
        }
        let state = self.snapshot.as_ref().ok_or(SessionError::NotJoined)?;
        if self.you.is_none() || state.presenter != self.you {
            return Err(SessionError::Unauthorized);
        }
        let same_context = self.published_camera.as_ref().is_some_and(|c| {
            c.room == state.room
                && c.epoch == state.camera_epoch
                && c.scene_revision == state.document.revision
        });
        if same_context
            && self.published_camera.as_ref().is_some_and(|c| {
                c.pose == pose
                    || now.saturating_sub(self.camera_published_at) < Duration::from_millis(50)
            })
        {
            return Ok(false);
        }
        let sequence = if same_context {
            self.published_camera
                .as_ref()
                .map_or(1, |c| c.sequence.saturating_add(1))
        } else {
            1
        };
        let sample = CameraSnapshot {
            room: state.room.clone(),
            epoch: state.camera_epoch,
            scene_revision: state.document.revision,
            sequence,
            pose,
        };
        self.send(ClientRequest::PublishCamera(sample.clone()))?;
        self.published_camera = Some(sample);
        self.camera_published_at = now;
        Ok(true)
    }
    /// Return the private credential for restoring this participant identity.
    pub fn resume_credential(&self) -> Option<&ResumeCredential> {
        self.resume_credential.as_ref()
    }
    /// Restore a private credential before connecting to its issuing authority.
    pub fn restore_resume_credential(&mut self, credential: ResumeCredential) {
        self.resume_credential = Some(credential);
    }
    /// Queue an explicit Leave; wait for Left before disconnecting the transport.
    pub fn leave(&mut self) -> Result<(), SessionError> {
        self.send(ClientRequest::Leave)
    }

    /// Return the local cursor subscription and its generation.
    pub fn cursor_subscription(&self) -> Option<&CursorSubscription> {
        self.cursors.subscription.as_ref()
    }
    /// Identifies this local transport lifetime, including reconnects.
    pub fn connection_id(&self) -> Option<Entity> {
        self.entity
    }
    /// Local discontinuity token, not a wire timestamp or persistent identity.
    pub fn remote_cursor_stream(&self, participant: ParticipantId) -> Option<u64> {
        self.cursors.streams.get(&participant).copied()
    }
    /// Iterate the currently retained remote cursor samples.
    pub fn remote_cursors(&self) -> impl Iterator<Item = &RemoteCursor> {
        self.cursors.remote.values()
    }
    /// Report actual local following on the reliable control lane. Safe to call
    /// every frame: unchanged context/value is deduplicated, unsent values are
    /// coalesced, and a full queue can be retried without losing the update.
    pub fn set_following(&mut self, following: bool) -> Result<(), SessionError> {
        let target = if following {
            Some(
                self.snapshot
                    .as_ref()
                    .ok_or(SessionError::NotJoined)?
                    .presenter
                    .ok_or(SessionError::Unauthorized)?,
            )
        } else {
            None
        };
        self.set_follow_target(target)
    }

    /// Reliably publish a personal follow choice, coalescing unsent choices.
    pub fn set_follow_target(&mut self, target: Option<ParticipantId>) -> Result<(), SessionError> {
        let state = self.snapshot.as_ref().ok_or(SessionError::NotJoined)?;
        let you = self.you.ok_or(SessionError::NotJoined)?;
        if !state.can_follow(you, target) {
            return Err(SessionError::Unauthorized);
        }
        if self.following.is_none()
            && state
                .participants
                .iter()
                .any(|p| p.id == you && p.following == target)
        {
            return Ok(());
        }
        let next = (
            state.room.clone(),
            state.presenter,
            state.camera_epoch,
            target,
        );
        if self.following.as_ref() == Some(&next) {
            return Ok(());
        }
        self.queue
            .retain(|r| !matches!(r, ClientRequest::SetFollowing { .. }));
        self.send(ClientRequest::SetFollowing {
            room: next.0.clone(),
            presenter: next.1,
            epoch: next.2,
            target,
        })?;
        self.following = Some(next);
        Ok(())
    }

    /// Publish a rendered view independently of cursor visibility, capped at 20 Hz.
    /// Repeat settled views each second so late arrivals and dropped final packets recover.
    pub fn publish_view(&mut self, pose: CameraPose, now: Duration) -> Result<bool, SessionError> {
        let state = self.snapshot.as_ref().ok_or(SessionError::NotJoined)?;
        if self.status != ConnectionStatus::Joined || self.you.is_none() {
            return Err(SessionError::NotJoined);
        }
        if !D::registrations().has_camera() || !valid_pose(&pose) {
            return Err(SessionError::Invalid);
        }
        let same = self.published_view.as_ref().is_some_and(|old| {
            old.room == state.room
                && old.epoch == state.camera_epoch
                && old.scene_revision == state.document.revision
        });
        let elapsed = now.saturating_sub(self.view_published_at);
        if same
            && (elapsed < Duration::from_millis(50)
                || (elapsed < Duration::from_secs(1)
                    && self
                        .published_view
                        .as_ref()
                        .is_some_and(|old| old.pose == pose)))
        {
            return Ok(false);
        }
        let sequence = if same {
            self.published_view
                .as_ref()
                .unwrap()
                .sequence
                .saturating_add(1)
        } else {
            1
        };
        let camera = CameraSnapshot {
            room: state.room.clone(),
            epoch: state.camera_epoch,
            scene_revision: state.document.revision,
            sequence,
            pose,
        };
        self.outgoing_view = Some(camera.clone());
        self.published_view = Some(camera);
        self.view_published_at = now;
        Ok(true)
    }

    /// Latest validated rendered view for a live peer in the current room context.
    pub fn participant_view(&self, participant: ParticipantId) -> Option<&CameraSnapshot> {
        let state = self.snapshot.as_ref()?;
        if !state.participants.iter().any(|p| p.id == participant) {
            return None;
        }
        self.views.get(&participant).filter(|c| {
            c.room == state.room
                && c.epoch == state.camera_epoch
                && c.scene_revision == state.document.revision
        })
    }

    /// Enable or retire local cursor sharing and queue the subscription change.
    pub fn set_cursor_sharing(&mut self, enabled: bool) -> Result<(), SessionError> {
        let room = self
            .snapshot
            .as_ref()
            .ok_or(SessionError::NotJoined)?
            .room
            .clone();
        let unchanged = self
            .cursors
            .subscription
            .as_ref()
            .is_some_and(|s| s.room == room && s.enabled == enabled);
        if !unchanged {
            self.cursors.generation = self.cursors.generation.saturating_add(1);
            self.cursors.subscription = Some(CursorSubscription {
                room,
                enabled,
                generation: self.cursors.generation,
            });
            self.cursors.delivery = SubscriptionDelivery::Pending;
            self.cursors.outgoing = None;
            self.cursors.remote.clear();
            self.cursors.streams.clear();
            self.queue
                .retain(|r| !matches!(r, ClientRequest::SetCursorSubscription(_)));
        }
        if self.cursors.delivery != SubscriptionDelivery::Pending {
            return Ok(());
        }
        let subscription = self.cursors.subscription.as_ref().unwrap().clone();
        self.send(ClientRequest::SetCursorSubscription(subscription))?;
        self.cursors.delivery = SubscriptionDelivery::AwaitingAck;
        Ok(())
    }
    /// Publish a screen-relative cursor for consumers sharing the same view.
    pub fn publish_cursor(&mut self, position: Option<[f32; 2]>) -> Result<(), SessionError> {
        self.publish_presence(position, None)
    }
    /// Publish a camera and optional world pointer without taking presentation control.
    pub fn publish_spatial_presence(
        &mut self,
        spatial: SpatialPresence,
    ) -> Result<(), SessionError> {
        self.publish_presence(None, Some(spatial))
    }
    /// Publish screen and world representations together in one coalesced presence sample.
    pub fn publish_presence(
        &mut self,
        position: Option<[f32; 2]>,
        spatial: Option<SpatialPresence>,
    ) -> Result<(), SessionError> {
        let room = self.snapshot.as_ref().ok_or(SessionError::NotJoined)?;
        if self.cursors.delivery != SubscriptionDelivery::Confirmed {
            return Err(SessionError::NotJoined);
        }
        let subscription = self
            .cursors
            .subscription
            .as_ref()
            .filter(|s| s.enabled)
            .ok_or(SessionError::Unauthorized)?;
        let sample = CursorSample {
            room: room.room.clone(),
            epoch: room.camera_epoch,
            scene_revision: room.document.revision,
            generation: subscription.generation,
            sequence: self.cursors.sequence.saturating_add(1),
            position,
            spatial,
        };
        self.send(ClientRequest::PublishCursor(sample))
    }
    /// Bounded reliable commands; camera samples coalesce to the newest sample.
    pub fn send(&mut self, request: ClientRequest<D>) -> Result<(), SessionError> {
        if self.status == ConnectionStatus::Disconnected {
            return Err(SessionError::Transport);
        }
        if let ClientRequest::PublishCursor(sample) = request {
            if self.cursors.delivery != SubscriptionDelivery::Confirmed
                || !sample.valid()
                || !self.cursors.subscription.as_ref().is_some_and(|s| {
                    s.enabled && s.room == sample.room && s.generation == sample.generation
                })
            {
                return Err(SessionError::Invalid);
            }
            self.cursors.sequence = sample.sequence;
            self.cursors.outgoing = Some(sample);
            return Ok(());
        }
        if let ClientRequest::PublishView(camera) = request {
            self.outgoing_view = Some(camera);
            return Ok(());
        }
        if let ClientRequest::PublishCamera(camera) = request {
            self.camera = Some(camera);
            return Ok(());
        }
        if self.queue.len() + self.pending >= CLIENT_QUEUE_CAPACITY {
            return Err(SessionError::QueueFull);
        }
        match &request {
            ClientRequest::Resume { credential } => {
                self.resuming = true;
                self.joining_room = Some(credential.room().to_owned());
                self.remote_camera = None;
            }
            ClientRequest::Join { room, .. } => {
                self.joining_room = Some(room.clone());
                self.remote_camera = None;
            }
            ClientRequest::Create { .. }
            | ClientRequest::JoinWithCode { .. }
            | ClientRequest::Leave => {
                self.joining_room = None;
                self.remote_camera = None;
            }
            _ => {}
        }
        self.queue.push_back(request);
        Ok(())
    }
    /// Count queued controls and coalesced samples awaiting transmission.
    pub fn pending_count(&self) -> usize {
        self.queue.len()
            + usize::from(self.camera.is_some())
            + usize::from(self.outgoing_view.is_some())
            + usize::from(self.cursors.outgoing.is_some())
    }
    fn clear(&mut self) {
        self.queue.clear();
        self.presenter_events.clear();
        self.spotlight = None;
        self.pending_document = None;
        self.document_edit_outcome = None;
        self.published_camera = None;
        self.published_view = None;
        self.outgoing_view = None;
        self.views.clear();
        self.cursors = default();
        self.following = None;
        self.pending = 0;
        self.remote_camera = None;
        self.joining_room = None;
        self.camera = None;
        self.you = None;
        self.snapshot = None;
        self.invitations = None;
    }
    /// Apply server data with version/epoch/revision guards, also useful to adapters in tests.
    pub fn receive(&mut self, message: ServerMessage<D>) {
        match message {
            ServerMessage::Invitations(invitations) => {
                if self.status == ConnectionStatus::Joined
                    && self
                        .snapshot
                        .as_ref()
                        .is_some_and(|s| self.you == Some(s.owner))
                {
                    self.invitations = Some(invitations);
                }
            }
            ServerMessage::Probe { .. } => {}
            ServerMessage::PresenterDecisionRejected { request, error } => {
                self.error = Some(error);
                if self.presenter_events.len() == 32 {
                    self.presenter_events.pop_front();
                }
                if let Some(snapshot) = &self.snapshot {
                    self.presenter_events.push_back(PresenterEvent::Rejected {
                        room: snapshot.room.clone(),
                        request,
                        error,
                    });
                }
            }
            ServerMessage::PresenterRequested(request) => {
                if self.presenter_events.len() == 32 {
                    self.presenter_events.pop_front();
                }
                if let Some(snapshot) = &self.snapshot {
                    self.presenter_events.push_back(PresenterEvent::Requested {
                        room: snapshot.room.clone(),
                        request,
                    });
                }
            }
            ServerMessage::PresenterDecision { request, granted } => {
                if self.presenter_events.len() == 32 {
                    self.presenter_events.pop_front();
                }
                if let Some(snapshot) = &self.snapshot {
                    self.presenter_events.push_back(PresenterEvent::Decision {
                        room: snapshot.room.clone(),
                        request,
                        granted,
                    });
                }
            }
            ServerMessage::DocumentEditOutcome(outcome) => {
                if self.pending_document == Some(outcome.id()) {
                    self.pending_document = None;
                    self.queue.retain(|r| !matches!(r, ClientRequest::SubmitDocument { id, .. } if *id == outcome.id()));
                    self.document_edit_outcome = Some(outcome);
                }
            }
            ServerMessage::Spotlight {
                room,
                presenter,
                epoch,
            } => {
                if self.status == ConnectionStatus::Joined
                    && self.snapshot.as_ref().is_some_and(|s| {
                        s.room == room && s.presenter == Some(presenter) && s.camera_epoch == epoch
                    })
                {
                    self.spotlight = Some((room, presenter, epoch));
                    // The authority cleared confirmed presence; publish the resulting view again.
                    self.following = None;
                    self.queue
                        .retain(|r| !matches!(r, ClientRequest::SetFollowing { .. }));
                }
            }
            ServerMessage::Heartbeat => {}
            ServerMessage::ResumeCredential(credential) => {
                if self.you == Some(credential.participant())
                    && self
                        .snapshot
                        .as_ref()
                        .is_some_and(|s| s.room == credential.room())
                {
                    self.resume_credential = Some(credential);
                }
            }
            ServerMessage::Replaced => {
                self.clear();
                self.resume_credential = None;
                self.status = ConnectionStatus::Disconnected;
                self.error = Some(SessionError::Replaced);
            }

            ServerMessage::CursorSubscription(subscription) => {
                if self.cursors.subscription.as_ref() == Some(&subscription) {
                    self.cursors.delivery = SubscriptionDelivery::Confirmed;
                }
            }
            ServerMessage::Cursor(cursor) => {
                if let Some(room) = &self.snapshot {
                    self.cursors.receive(cursor, room, self.you);
                }
            }
            ServerMessage::Welcome {
                you,
                invitations,
                snapshot,
            } => {
                if !valid_snapshot(&snapshot) || !snapshot.participants.iter().any(|p| p.id == you)
                {
                    self.receive(ServerMessage::Rejected(SessionError::Invalid));
                    return;
                }
                if self.status == ConnectionStatus::Joined {
                    return;
                }
                self.resuming = false;
                self.joining_room = None;
                self.cursors.reset_room();
                self.following = None;
                self.you = Some(you);
                self.invitations = invitations;
                self.snapshot = Some(snapshot);
                self.status = ConnectionStatus::Joined;
                self.error = None;
                if let Some(camera) = self.remote_camera.take() {
                    self.receive(ServerMessage::Camera(camera));
                }
            }
            ServerMessage::Snapshot(mut next) => {
                if self.status != ConnectionStatus::Joined || self.snapshot.is_none() {
                    return;
                }
                if !valid_snapshot(&next) {
                    self.receive(ServerMessage::Rejected(SessionError::Invalid));
                    return;
                }
                if let Some(old) = &self.snapshot {
                    if next.room != old.room
                        || next.version <= old.version
                        || next.camera_epoch < old.camera_epoch
                        || next.document.revision < old.document.revision
                    {
                        return;
                    }
                    if let Some(camera) = &old.camera
                        && camera.room == next.room
                        && camera.epoch == next.camera_epoch
                        && camera.scene_revision == next.document.revision
                        && next
                            .camera
                            .as_ref()
                            .is_none_or(|c| c.sequence < camera.sequence)
                    {
                        next.camera = Some(camera.clone());
                    }
                }
                if self
                    .following
                    .as_ref()
                    .is_some_and(|(room, presenter, epoch, _)| {
                        room != &next.room
                            || presenter != &next.presenter
                            || *epoch != next.camera_epoch
                    })
                    || (next.forced_follow
                        && self.snapshot.as_ref().is_some_and(|old| !old.forced_follow))
                {
                    self.following = None;
                    self.queue
                        .retain(|r| !matches!(r, ClientRequest::SetFollowing { .. }));
                }
                self.cursors.reconcile(&next);
                self.views.retain(|id, camera| {
                    next.participants.iter().any(|p| p.id == *id)
                        && camera.room == next.room
                        && camera.epoch == next.camera_epoch
                        && camera.scene_revision == next.document.revision
                });
                self.snapshot = Some(next);
                if let Some(camera) = self.remote_camera.take() {
                    self.receive(ServerMessage::Camera(camera));
                }
            }
            ServerMessage::View {
                participant,
                camera,
            } => {
                let Some(state) = self.snapshot.as_ref() else {
                    return;
                };
                if self.status != ConnectionStatus::Joined
                    || !valid_pose(&camera.pose)
                    || Some(participant) == self.you
                    || !state.participants.iter().any(|p| p.id == participant)
                    || camera.room != state.room
                    || camera.epoch != state.camera_epoch
                    || camera.scene_revision != state.document.revision
                {
                    return;
                }
                if self.views.get(&participant).is_none_or(|old| {
                    old.epoch != camera.epoch
                        || old.scene_revision != camera.scene_revision
                        || old.sequence < camera.sequence
                }) {
                    self.views.insert(participant, camera);
                }
            }
            ServerMessage::Camera(camera) => {
                if !valid_pose(&camera.pose) {
                    return;
                }
                if let Some(joining) = &self.joining_room {
                    if &camera.room == joining
                        && self.remote_camera.as_ref().is_none_or(|old| {
                            old.room != camera.room
                                || (camera.epoch, camera.scene_revision, camera.sequence)
                                    > (old.epoch, old.scene_revision, old.sequence)
                        })
                    {
                        self.remote_camera = Some(camera);
                    }
                    return;
                }
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.room != camera.room)
                {
                    return;
                }
                if self.snapshot.is_none() && self.joining_room.as_ref() != Some(&camera.room) {
                    return;
                }
                let future = self.snapshot.as_ref().is_none_or(|s| {
                    camera.epoch > s.camera_epoch
                        || (camera.epoch == s.camera_epoch
                            && camera.scene_revision > s.document.revision)
                });
                if future && self.status != ConnectionStatus::Disconnected {
                    if self.remote_camera.as_ref().is_none_or(|old| {
                        (camera.epoch, camera.scene_revision, camera.sequence)
                            > (old.epoch, old.scene_revision, old.sequence)
                    }) {
                        self.remote_camera = Some(camera);
                    }
                    return;
                }
                if let Some(state) = &mut self.snapshot
                    && camera.epoch == state.camera_epoch
                    && camera.scene_revision == state.document.revision
                    && state
                        .camera
                        .as_ref()
                        .is_none_or(|old| camera.sequence > old.sequence)
                    && valid_pose(&camera.pose)
                {
                    state.camera = Some(camera);
                }
            }
            ServerMessage::Rejected(error) => {
                self.following = None;
                self.error = Some(
                    if self.resuming
                        && matches!(error, SessionError::Unauthorized | SessionError::Invalid)
                    {
                        self.resume_credential = None;
                        self.resuming = false;
                        SessionError::ResumeRejected
                    } else {
                        error
                    },
                );
                self.resuming = false;
                self.joining_room = None;
                self.remote_camera = None;
                self.queue.retain(|r| {
                    matches!(
                        r,
                        ClientRequest::SetCursorSubscription(_)
                            | ClientRequest::SubmitDocument { .. }
                    )
                });
                self.camera = None;
            }
            ServerMessage::Left => {
                // Discarded submissions can no longer receive an outcome. Keep
                // next_edit monotonic so a delayed old outcome cannot alias a new edit.
                self.pending_document = None;
                self.document_edit_outcome = None;
                self.published_camera = None;
                self.published_view = None;
                self.outgoing_view = None;
                self.views.clear();
                self.resume_credential = None;
                self.cursors.reset_room();
                self.following = None;
                // Leaving a room does not reset this socket's reliable ACK accounting.
                // A Join queued while Leave was in flight still needs to be delivered.
                let joining = self.joining_room.clone();
                self.queue.retain(
                    |r| matches!(r,ClientRequest::Join{room,..} if Some(room)==joining.as_ref()),
                );
                self.remote_camera = self
                    .remote_camera
                    .take()
                    .filter(|c| joining.as_ref() == Some(&c.room));
                self.camera = None;
                self.you = None;
                self.snapshot = None;
                self.invitations = None;
                self.status = ConnectionStatus::Connected;
            }
        }
    }
}
/// App adapters can run after this set to consume the current received snapshot.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollaborationSystems {
    Transport,
}
#[derive(Default)]
pub struct CollaborationClientPlugin<D: SharedDocument>(std::marker::PhantomData<D>);
impl<D: SharedDocument> Plugin for CollaborationClientPlugin<D> {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            dimensify_transport::client::TransportClientPlugin,
            ProtocolPlugin::<D>::default(),
        ))
        .init_resource::<ClientSession<D>>()
        .init_resource::<EndpointProbe<D>>()
        .add_systems(
            Update,
            (
                crate::probe::pump::<D>,
                prepare_transport::<D>,
                pump_client::<D>,
            )
                .chain()
                .in_set(CollaborationSystems::Transport),
        );
    }
}
/// Connect using the same API on native and browser WASM. Install the plugin first.
pub fn connect<D: SharedDocument>(world: &mut World, endpoint: &str) -> Result<(), SessionError> {
    let endpoint = Endpoint::parse(endpoint)?;
    let normalized = endpoint.websocket.clone();
    let previous = world.resource::<ClientSession<D>>();
    // Explicit restoration on a fresh session is trusted; reconnects are endpoint bound.
    let credential = if previous
        .last_endpoint
        .as_ref()
        .is_none_or(|old| *old == normalized)
    {
        previous.resume_credential.clone()
    } else {
        None
    };
    disconnect::<D>(world);
    let attempt = Attempt::start::<crate::transport::SessionHandshake<D>>(world, endpoint);
    let entity = attempt.entity;
    let started = world.resource::<Time<Real>>().elapsed();
    *world.resource_mut::<ClientSession<D>>() = ClientSession {
        status: ConnectionStatus::Connecting,
        entity: Some(entity),
        attempt: Some(attempt),
        started,
        last_ack: started,
        last_received: started,
        last_heartbeat: started,
        resume_credential: credential,
        last_endpoint: Some(normalized),
        ..default()
    };
    Ok(())
}
/// Local teardown that forgets credentials. Use `leave` to revoke server membership.
pub fn disconnect<D: SharedDocument>(world: &mut World) {
    let entity = world.resource::<ClientSession<D>>().entity;
    if let Some(entity) = entity {
        crate::transport::close(world, entity);
    }
    *world.resource_mut::<ClientSession<D>>() = ClientSession::default();
}
fn prepare_transport<D: SharedDocument>(world: &mut World) {
    if world.resource::<ClientSession<D>>().status != ConnectionStatus::Connecting {
        return;
    }
    let Some(mut attempt) = world.resource_mut::<ClientSession<D>>().attempt.take() else {
        return;
    };
    let was_sent = attempt.sent;
    let outcome = attempt.poll::<crate::transport::SessionHandshake<D>>(world);
    let now = world.resource::<Time<Real>>().elapsed();
    let mut session = world.resource_mut::<ClientSession<D>>();
    session.transport_failures.clone_from(&attempt.failures);
    match outcome {
        AttemptStatus::Pending | AttemptStatus::Ready => {
            // Probe uses the same reliable lane. Include its ACK in accounting so
            // a late probe ACK cannot acknowledge a queued application command.
            if session.entity != Some(attempt.entity) {
                session.pending = usize::from(attempt.sent);
                session.last_ack = now;
            } else if !was_sent && attempt.sent {
                session.pending += 1;
                session.last_ack = now;
            }
            session.entity = Some(attempt.entity);
            session.attempt = Some(attempt);
        }
        AttemptStatus::Failed => {
            session.clear();
            session.entity = None;
            session.status = ConnectionStatus::Disconnected;
            session.error = Some(SessionError::Transport);
        }
    }
}
#[allow(
    clippy::type_complexity,
    reason = "Keep the optional transport components explicit at the ECS system boundary"
)]
fn pump_client<D: SharedDocument>(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut session: ResMut<ClientSession<D>>,
    mut peers: Query<(
        Option<&Connected>,
        Option<&ProtocolReady>,
        Option<&Unlinked>,
        &mut MessageSender<ClientEnvelope<D>>,
        Option<&mut MessageReceiver<ServerEnvelope<D>>>,
        Option<&Transport>,
    )>,
) {
    let Some(entity) = session.entity else { return };
    let Ok((connected, ready, unlinked, mut sender, mut receiver, transport)) =
        peers.get_mut(entity)
    else {
        session.clear();
        session.status = ConnectionStatus::Disconnected;
        session.entity = None;
        session.error = Some(SessionError::Transport);
        return;
    };
    if let Some(channel) =
        transport.and_then(|t| t.channel_send(ChannelKind::of::<ControlChannel>()))
    {
        let count = channel.message_acks().len();
        if count > 0 {
            session.pending = session.pending.saturating_sub(count);
            session.last_ack = time.elapsed();
        }
    }
    if session.pending > 0
        && time.elapsed().saturating_sub(session.last_ack) > Duration::from_secs(15)
    {
        session.clear();
        session.status = ConnectionStatus::Disconnected;
        session.entity = None;
        session.error = Some(SessionError::Transport);
        commands.trigger(Disconnect { entity });
        commands.entity(entity).despawn();
        return;
    }
    if connected.is_none() || ready.is_none() {
        if matches!(
            session.status,
            ConnectionStatus::Joined | ConnectionStatus::Connected
        ) || unlinked.is_some()
            || time.elapsed().saturating_sub(session.started) > Duration::from_secs(15)
        {
            session.clear();
            session.status = ConnectionStatus::Disconnected;
            session.error = Some(SessionError::Transport);
            session.entity = None;
            commands.entity(entity).despawn();
        }
        return;
    }
    if session.status == ConnectionStatus::Connecting {
        session.status = ConnectionStatus::Connected;
        if !session
            .queue
            .iter()
            .any(|r| matches!(r, ClientRequest::Resume { .. }))
            && let Some(credential) = session.resume_credential.clone()
        {
            let _ = session.send(ClientRequest::Resume { credential });
        }
    }
    if let Some(receiver) = receiver.as_mut() {
        for message in receiver.receive() {
            session.last_received = time.elapsed();
            session.receive(message.message);
        }
    }
    if session.status == ConnectionStatus::Disconnected {
        commands.trigger(Disconnect { entity });
        commands.entity(entity).despawn();
        session.entity = None;
        return;
    }
    if session.status == ConnectionStatus::Joined {
        if time.elapsed().saturating_sub(session.last_received) > Duration::from_secs(30) {
            session.clear();
            session.status = ConnectionStatus::Disconnected;
            session.entity = None;
            session.error = Some(SessionError::Transport);
            commands.trigger(Disconnect { entity });
            commands.entity(entity).despawn();
            return;
        }
        if time.elapsed().saturating_sub(session.last_heartbeat) >= Duration::from_secs(5)
            && session.send(ClientRequest::Heartbeat).is_ok()
        {
            session.last_heartbeat = time.elapsed();
        }
    }
    while session.pending < CLIENT_QUEUE_CAPACITY {
        let Some(request) = session.queue.pop_front() else {
            break;
        };
        if session.pending == 0 {
            session.last_ack = time.elapsed();
        }
        session.pending += 1;
        sender.send::<ControlChannel>(Envelope::new(request));
    }
    if let Some(camera) = session.outgoing_view.take() {
        sender.send::<CursorChannel>(Envelope::new(ClientRequest::PublishView(camera)));
    }
    if let Some(cursor) = session.cursors.outgoing.take() {
        sender.send::<CursorChannel>(Envelope::new(ClientRequest::PublishCursor(cursor)));
    }
    if let Some(camera) = session.camera.take() {
        sender.send::<CameraChannel>(Envelope::new(ClientRequest::PublishCamera(camera)));
    }
}

#[cfg(test)]
mod transport_readiness_tests {
    use super::*;
    use crate::protocol::tests::TestDocument;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn enabling_lock_republishes_an_already_following_clients_presence() {
        let mut state = RoomSnapshot::<TestDocument> {
            room: "room".into(),
            version: 1,
            document: Default::default(),
            owner: 1,
            participants: vec![
                Participant {
                    id: 1,
                    name: "Presenter".into(),
                    role: Role::Editor,
                    following: None,
                },
                Participant {
                    id: 2,
                    name: "Viewer".into(),
                    role: Role::Viewer,
                    following: None,
                },
            ],
            presenter: Some(1),
            presenter_requests: vec![],
            camera_epoch: 1,
            forced_follow: false,
            camera: None,
        };
        let mut session = ClientSession::<TestDocument>::default();
        session.receive(ServerMessage::Welcome {
            you: 2,
            invitations: None,
            snapshot: state.clone(),
        });
        session.set_following(true).unwrap();
        session.queue.clear(); // The transport has delivered the confirmation.
        state.version += 1;
        state.forced_follow = true; // The authority clears old confirmations when locking.
        session.receive(ServerMessage::Snapshot(state));
        session.set_following(true).unwrap();
        assert_eq!(
            session.pending_count(),
            1,
            "locked followers must confirm again"
        );
    }

    #[test]
    fn cancelled_handshakes_leave_no_retired_frontends() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CollaborationClientPlugin::<TestDocument>::default());
        app.finish();
        app.cleanup();
        for _ in 0..4 {
            connect::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
            assert_eq!(
                app.world()
                    .resource::<ClientSession<TestDocument>>()
                    .transport_kind(),
                Some(TransportKind::WebTransport)
            );
            disconnect::<TestDocument>(app.world_mut());
            assert_eq!(
                app.world_mut()
                    .query::<&aeronet_webtransport::client::WebTransportClient>()
                    .iter(app.world())
                    .count(),
                0
            );
        }
    }

    #[test]
    fn automatic_recovery_stays_on_websocket_after_quic_fallback() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CollaborationClientPlugin::<TestDocument>::default());
        app.finish();
        app.cleanup();
        connect::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(3));
        prepare_transport::<TestDocument>(app.world_mut());
        assert_eq!(
            app.world()
                .resource::<ClientSession<TestDocument>>()
                .transport_kind(),
            Some(TransportKind::WebSocket)
        );
        disconnect::<TestDocument>(app.world_mut());
        connect::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        assert_eq!(
            app.world()
                .resource::<ClientSession<TestDocument>>()
                .transport_kind(),
            Some(TransportKind::WebSocket),
            "recovery must not launch another QUIC handshake"
        );
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(8));
        prepare_transport::<TestDocument>(app.world_mut());
        assert_eq!(
            app.world().resource::<ClientSession<TestDocument>>().status,
            ConnectionStatus::Disconnected
        );
        crate::retry_webtransport_on_next_connect(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        connect::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        assert_eq!(
            app.world()
                .resource::<ClientSession<TestDocument>>()
                .transport_kind(),
            Some(TransportKind::WebTransport),
            "explicit reconnect may retry QUIC after the failed connection"
        );
    }

    #[test]
    fn transport_handshake_does_not_send_room_commands_before_protocol_readiness() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(CollaborationClientPlugin::<TestDocument>::default());
        app.finish();
        app.cleanup();
        connect::<TestDocument>(app.world_mut(), "wss://127.0.0.1:9/").unwrap();
        let entity = app
            .world()
            .resource::<ClientSession<TestDocument>>()
            .entity
            .unwrap();
        app.world_mut()
            .entity_mut(entity)
            .insert((RemoteId(PeerId::Netcode(1)), Connected));
        app.world_mut()
            .resource_mut::<ClientSession<TestDocument>>()
            .send(ClientRequest::Create {
                name: "host".into(),
            })
            .unwrap();
        app.world_mut()
            .run_system_once(pump_client::<TestDocument>)
            .unwrap();
        let session = app.world().resource::<ClientSession<TestDocument>>();
        assert_eq!(session.status, ConnectionStatus::Connecting);
        assert_eq!(
            session.pending_count(),
            1,
            "room command waits for a protocol round trip"
        );
    }
}
