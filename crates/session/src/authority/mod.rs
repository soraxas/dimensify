//! Pure room authority. `now` is monotonic seconds supplied by the transport.
use crate::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct Limits {
    pub rooms: usize,
    pub participants: usize,
    pub empty_room_seconds: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            rooms: 64,
            participants: MAX_PARTICIPANTS,
            empty_room_seconds: 1800,
        }
    }
}
pub(crate) struct Room<D> {
    pub(crate) state: RoomSnapshot<D>,
    pub(crate) invitations: Invitations,
    pub(crate) codes_expire_at: u64,
    pub(crate) empty_since: Option<u64>,
    pub(crate) cursors: crate::cursors::CursorRoom,
    pub(crate) views: BTreeMap<ParticipantId, CameraSnapshot>,
    view_sequence: u64,
    pub(crate) resumes: BTreeMap<ParticipantId, crate::resume::ResumeRecord>,
}
pub struct Authority<D> {
    pub limits: Limits,
    pub(crate) rooms: BTreeMap<RoomId, Room<D>>,
    pub(crate) members: BTreeMap<ParticipantId, RoomId>,
    pub(crate) connections: BTreeMap<u64, ParticipantId>,
    pub(crate) next_identity: u64,
}
impl<D: SharedDocument> Default for Authority<D> {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            rooms: BTreeMap::new(),
            members: BTreeMap::new(),
            connections: BTreeMap::new(),
            next_identity: 1,
        }
    }
}
/// Generate a cryptographically random private capability token.
pub(crate) fn token() -> Result<String, SessionError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| SessionError::Transport)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
/// Create a snapshot message for each current room participant.
pub(crate) fn broadcast<D: SharedDocument>(room: &Room<D>) -> Vec<(u64, ServerMessage<D>)> {
    room.state
        .participants
        .iter()
        .map(|p| (p.id, ServerMessage::Snapshot(room.state.clone())))
        .collect()
}
impl<D: SharedDocument> Authority<D> {
    /// Check the destination transport and cursor subscription are still live.
    pub(crate) fn accepts_cursor(&self, peer: u64, cursor: &RemoteCursor) -> bool {
        let Some(peer) = self.connections.get(&peer).copied() else {
            return false;
        };
        self.members
            .get(&peer)
            .and_then(|id| self.rooms.get(id))
            .is_some_and(|room| {
                cursor.sample.matches(&room.state)
                    && room.cursors.accepts(peer, cursor.subscription)
            })
    }
    /// Report whether this transport has a room identity.
    pub fn is_joined(&self, peer: u64) -> bool {
        self.connections.contains_key(&peer)
    }
    /// Retire expired resume credentials and abandoned rooms.
    pub fn expire(&mut self, now: u64) {
        for room in self.rooms.values_mut() {
            room.resumes.retain(|_, r| {
                !r.dormant_since
                    .is_some_and(|t| now.saturating_sub(t) >= self.limits.empty_room_seconds)
            });
        }
        self.rooms.retain(|_, r| {
            !r.empty_since
                .is_some_and(|t| now.saturating_sub(t) >= self.limits.empty_room_seconds)
        });
    }
    /// Remove an identity from live participation and update room authority.
    pub(crate) fn disconnect_identity(
        &mut self,
        peer: u64,
        now: u64,
    ) -> Vec<(u64, ServerMessage<D>)> {
        let Some(id) = self.members.remove(&peer) else {
            return vec![];
        };
        let Some(room) = self.rooms.get_mut(&id) else {
            return vec![];
        };
        if let Some(record) = room.resumes.get_mut(&peer) {
            if let Some(participant) = room.state.participants.iter().find(|p| p.id == peer) {
                record.participant = participant.clone();
            }
            record.dormant_since = Some(now);
            record.presenter_epoch =
                (room.state.presenter == Some(peer)).then_some(room.state.camera_epoch + 1);
            record.camera = room.state.camera.clone();
        }
        room.cursors.remove(peer);
        room.views.remove(&peer);
        for participant in &mut room.state.participants {
            if participant.following == Some(peer) {
                participant.following = None;
            }
        }
        room.state.participants.retain(|p| p.id != peer);
        room.state
            .presenter_requests
            .retain(|p| p.participant != peer);
        if room.state.presenter == Some(peer) {
            room.state.presenter = None;
            room.state.camera_epoch += 1;
            room.state.camera = None;
            room.state.forced_follow = false;
            for participant in &mut room.state.participants {
                participant.following = None;
            }
        }
        room.state.version += 1;
        if room.state.participants.is_empty() {
            room.empty_since = Some(now);
        }
        broadcast(room)
    }
    /// Validate and apply a request using the stable participant identity.
    pub(crate) fn process_identity(
        &mut self,
        peer: u64,
        request: ClientRequest<D>,
        now: u64,
    ) -> Result<Vec<(u64, ServerMessage<D>)>, SessionError> {
        self.expire(now);
        if matches!(request, ClientRequest::RenewInviteCodes) {
            return self.renew_codes(peer, now);
        }
        let request = match request {
            ClientRequest::JoinWithCode { code, name } => {
                let (room, invitation) = self.resolve_code(&code, now)?;
                ClientRequest::Join {
                    room,
                    invitation,
                    name,
                }
            }
            request => request,
        };
        let request_kind = match &request {
            ClientRequest::GrantPresenter { .. } => Some(true),
            ClientRequest::DismissPresenter { .. } => Some(false),
            _ => None,
        };
        match request {
            ClientRequest::Create { name } => {
                if self.members.contains_key(&peer) {
                    return Err(SessionError::AlreadyJoined);
                }
                if !valid_display_name(&name) {
                    return Err(SessionError::Invalid);
                }
                if self.rooms.len() >= self.limits.rooms {
                    return Err(SessionError::Full);
                }
                D::registrations().descriptors()?;
                let document = Document::<D>::default();
                if !document.data.validate() {
                    return Err(SessionError::Invalid);
                }
                let id = token()?;
                let invitations = Invitations {
                    viewer: Invitation(token()?),
                    editor: Invitation(token()?),
                    codes: Some(self.new_codes()?),
                };
                let state = RoomSnapshot {
                    room: id.clone(),
                    version: 1,
                    document,
                    participants: vec![Participant {
                        id: peer,
                        name: name.trim().to_owned(),
                        role: Role::Editor,
                        following: None,
                    }],
                    owner: peer,
                    presenter: Some(peer),
                    presenter_requests: vec![],
                    camera_epoch: 1,
                    forced_follow: false,
                    camera: None,
                };
                self.members.insert(peer, id.clone());
                self.rooms.insert(
                    id,
                    Room {
                        state: state.clone(),
                        invitations: invitations.clone(),
                        codes_expire_at: now.saturating_add(INVITE_CODE_LIFETIME_SECONDS),
                        empty_since: None,
                        cursors: Default::default(),
                        views: BTreeMap::new(),
                        view_sequence: 0,
                        resumes: BTreeMap::new(),
                    },
                );
                return Ok(vec![(
                    peer,
                    ServerMessage::Welcome {
                        you: peer,
                        invitations: Some(invitations),
                        snapshot: state,
                    },
                )]);
            }
            ClientRequest::Join {
                room: id,
                invitation,
                name,
            } => {
                if self.members.contains_key(&peer) {
                    return Err(SessionError::AlreadyJoined);
                }
                if !valid_display_name(&name) {
                    return Err(SessionError::Invalid);
                }
                let room = self.rooms.get_mut(&id).ok_or(SessionError::Unauthorized)?;
                let role = if invitation == room.invitations.editor {
                    Role::Editor
                } else if invitation == room.invitations.viewer {
                    Role::Viewer
                } else {
                    return Err(SessionError::Unauthorized);
                };
                if room.resumes.len() >= self.limits.participants.min(MAX_PARTICIPANTS) {
                    return Err(SessionError::Full);
                }
                room.state.participants.push(Participant {
                    id: peer,
                    name: name.trim().to_owned(),
                    role,
                    following: None,
                });
                room.state.version += 1;
                room.empty_since = None;
                self.members.insert(peer, id);
                let mut messages = broadcast(room);
                messages.retain(|(p, _)| *p != peer);
                messages.push((
                    peer,
                    ServerMessage::Welcome {
                        you: peer,
                        invitations: None,
                        snapshot: room.state.clone(),
                    },
                ));
                return Ok(messages);
            }
            ClientRequest::Leave => {
                if !self.members.contains_key(&peer) {
                    return Err(SessionError::NotJoined);
                }
                let mut messages = self.disconnect_identity(peer, now);
                for room in self.rooms.values_mut() {
                    room.resumes.remove(&peer);
                }
                messages.push((peer, ServerMessage::Left));
                return Ok(messages);
            }
            _ => {}
        }
        let id = self.members.get(&peer).ok_or(SessionError::NotJoined)?;
        let room = self.rooms.get_mut(id).ok_or(SessionError::NotJoined)?;
        let state = &mut room.state;
        let editor = state
            .participants
            .iter()
            .any(|p| p.id == peer && p.role == Role::Editor);
        match request {
            ClientRequest::Heartbeat => return Ok(vec![(peer, ServerMessage::Heartbeat)]),
            ClientRequest::Rename { name } => {
                if !valid_display_name(&name) {
                    return Err(SessionError::Invalid);
                }
                let member = state
                    .participants
                    .iter_mut()
                    .find(|p| p.id == peer)
                    .ok_or(SessionError::NotJoined)?;
                member.name = name.trim().to_owned();
            }
            ClientRequest::SetParticipantRole { participant, role } => {
                if peer != state.owner || participant == state.owner {
                    return Err(SessionError::Unauthorized);
                }
                let member = state
                    .participants
                    .iter_mut()
                    .find(|p| p.id == participant)
                    .ok_or(SessionError::Invalid)?;
                member.role = role;
            }
            ClientRequest::SetFollowing {
                room,
                presenter,
                epoch,
                target,
            } => {
                if room != state.room || presenter != state.presenter || epoch != state.camera_epoch
                {
                    return Err(SessionError::Stale);
                }
                if !state.can_follow(peer, target) {
                    return Err(SessionError::Unauthorized);
                }
                let participant = state
                    .participants
                    .iter_mut()
                    .find(|p| p.id == peer)
                    .ok_or(SessionError::NotJoined)?;
                if participant.following == target {
                    return Ok(vec![]);
                }
                participant.following = target;
            }
            ClientRequest::SetCursorSubscription(subscription) => {
                return room.cursors.subscribe(peer, subscription, state);
            }
            ClientRequest::PublishCursor(sample) => {
                return room.cursors.publish(peer, sample, state);
            }
            ClientRequest::SetDocument {
                expected_revision,
                data,
            } => {
                let registrations = D::registrations();
                registrations.authorize(
                    &state.document.data,
                    &data,
                    editor,
                    state.presenter == Some(peer),
                )?;
                if expected_revision != state.document.revision {
                    return Err(SessionError::Conflict);
                }
                let data = registrations.apply(
                    &state.document.data,
                    &data,
                    editor,
                    state.presenter == Some(peer),
                )?;
                state.document = Document {
                    revision: expected_revision + 1,
                    data,
                };
                state.camera = None;
            }
            ClientRequest::RequestPresenter => {
                if state.presenter == Some(peer)
                    || state
                        .presenter_requests
                        .iter()
                        .any(|r| r.participant == peer)
                {
                    return Ok(vec![]);
                }
                let request = PresenterRequest {
                    participant: peer,
                    id: state.version + 1,
                };
                state.presenter_requests.push(request);
                state.version += 1;
                let mut messages = broadcast(room);
                messages.push((peer, ServerMessage::PresenterRequested(request)));
                return Ok(messages);
            }
            ClientRequest::GrantPresenter { request }
            | ClientRequest::DismissPresenter { request } => {
                let granted = matches!(&request_kind, Some(true));
                let participant = request.participant;
                if state.presenter != Some(peer) {
                    return Err(SessionError::Unauthorized);
                }
                if !state.participants.iter().any(|p| p.id == participant)
                    || !state.presenter_requests.contains(&request)
                {
                    return Err(SessionError::Stale);
                }
                state.presenter_requests.retain(|r| *r != request);
                if !granted {
                    state.version += 1;
                    let mut messages = broadcast(room);
                    messages.push((
                        participant,
                        ServerMessage::PresenterDecision { request, granted },
                    ));
                    messages.push((peer, ServerMessage::PresenterDecision { request, granted }));
                    return Ok(messages);
                }
                state.presenter = Some(participant);
                state
                    .presenter_requests
                    .retain(|p| p.participant != participant);
                state.camera_epoch += 1;
                state.camera = None;
                state.forced_follow = false;
                for participant in &mut state.participants {
                    participant.following = None;
                }
                state.version += 1;
                let mut messages = broadcast(room);
                messages.push((
                    participant,
                    ServerMessage::PresenterDecision { request, granted },
                ));
                messages.push((peer, ServerMessage::PresenterDecision { request, granted }));
                return Ok(messages);
            }
            ClientRequest::ClaimPresenter => {
                if !editor {
                    return Err(SessionError::Unauthorized);
                }
                if state.presenter.is_some() {
                    return Err(SessionError::Conflict);
                }
                state.presenter = Some(peer);
                state.presenter_requests.retain(|p| p.participant != peer);
                state.camera_epoch += 1;
                state.camera = None;
                state.forced_follow = false;
                for participant in &mut state.participants {
                    participant.following = None;
                }
            }
            ClientRequest::MakePresenter { participant } => {
                if state.presenter != Some(peer) {
                    return Err(SessionError::Unauthorized);
                }
                if !state.participants.iter().any(|p| p.id == participant) {
                    return Err(SessionError::Stale);
                }
                if participant == peer {
                    return Ok(vec![]);
                }
                state.presenter = Some(participant);
                state
                    .presenter_requests
                    .retain(|r| r.participant != participant);
                state.camera_epoch += 1;
                state.camera = None;
                state.forced_follow = false;
                for participant in &mut state.participants {
                    participant.following = None;
                }
            }
            ClientRequest::Spotlight => {
                if state.presenter != Some(peer) {
                    return Err(SessionError::Unauthorized);
                }
                // Clear chains first: even the presenter may have been following a peer.
                for participant in &mut state.participants {
                    participant.following = None;
                }
                state.version += 1;
                let notice = ServerMessage::Spotlight {
                    room: state.room.clone(),
                    presenter: peer,
                    epoch: state.camera_epoch,
                };
                let peers: Vec<_> = state.participants.iter().map(|p| p.id).collect();
                let mut messages = broadcast(room);
                messages.extend(peers.into_iter().map(|id| (id, notice.clone())));
                return Ok(messages);
            }
            ClientRequest::SetForcedFollow(value) => {
                if state.presenter != Some(peer) {
                    return Err(SessionError::Unauthorized);
                }
                if state.forced_follow == value {
                    return Ok(vec![]);
                }
                state.forced_follow = value;
                if value {
                    // Break existing chains before asking everyone to join the presenter.
                    for participant in &mut state.participants {
                        participant.following = None;
                    }
                }
            }
            ClientRequest::PublishView(camera) => {
                if !D::registrations().has_camera() || !valid_pose(&camera.pose) {
                    return Err(SessionError::Invalid);
                }
                if camera.room != state.room
                    || camera.epoch != state.camera_epoch
                    || camera.scene_revision != state.document.revision
                {
                    return Err(SessionError::Stale);
                }
                if room.views.get(&peer).is_some_and(|old| {
                    old.epoch == camera.epoch
                        && old.scene_revision == camera.scene_revision
                        && old.sequence >= camera.sequence
                }) {
                    return Err(SessionError::Stale);
                }
                room.views.insert(peer, camera.clone());
                // Authority sequencing survives an individual sender reconnecting with sequence 1.
                room.view_sequence = room.view_sequence.saturating_add(1);
                let mut camera = camera;
                camera.sequence = room.view_sequence;
                return Ok(state
                    .participants
                    .iter()
                    .filter(|p| p.id != peer)
                    .map(|p| {
                        (
                            p.id,
                            ServerMessage::View {
                                participant: peer,
                                camera: camera.clone(),
                            },
                        )
                    })
                    .collect());
            }
            ClientRequest::PublishCamera(camera) => {
                if !D::registrations().has_camera() || state.presenter != Some(peer) {
                    return Err(SessionError::Unauthorized);
                }
                if camera.room != state.room
                    || camera.epoch != state.camera_epoch
                    || camera.scene_revision != state.document.revision
                    || state
                        .camera
                        .as_ref()
                        .is_some_and(|old| camera.sequence <= old.sequence)
                {
                    return Err(SessionError::Stale);
                }
                if !valid_pose(&camera.pose) {
                    return Err(SessionError::Invalid);
                }
                state.camera = Some(camera.clone());
                return Ok(state
                    .participants
                    .iter()
                    .map(|p| (p.id, ServerMessage::Camera(camera.clone())))
                    .collect());
            }
            _ => unreachable!(),
        }
        state.version += 1;
        Ok(broadcast(room))
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    use crate::protocol::tests::TestDocument;
    #[test]
    fn resume_retains_owner_name_and_replaces_transport() {
        let mut a = Authority::<TestDocument>::default();
        let messages = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        let credential = messages
            .iter()
            .find_map(|(_, m)| match m {
                ServerMessage::ResumeCredential(c) => Some(c.clone()),
                _ => None,
            })
            .expect("private credential");
        a.process(
            1,
            ClientRequest::Rename {
                name: " Renamed ".into(),
            },
            1,
        )
        .unwrap();
        let resumed = a
            .process(
                9,
                ClientRequest::Resume {
                    credential: credential.clone(),
                },
                2,
            )
            .unwrap();
        let state = resumed
            .iter()
            .find_map(|(_, m)| match m {
                ServerMessage::Welcome { snapshot, .. } => Some(snapshot),
                _ => None,
            })
            .unwrap();
        assert_eq!(state.owner, 1);
        assert_eq!(state.participants[0].name, "Renamed");
        assert_eq!(
            a.process(
                1,
                ClientRequest::Rename {
                    name: "stale".into()
                },
                3
            ),
            Err(SessionError::NotJoined)
        );
        assert!(a.disconnect(1, 3).is_empty());
        assert!(a.is_joined(9));
        a.process(9, ClientRequest::Leave, 4).unwrap();
        assert_eq!(
            a.process(10, ClientRequest::Resume { credential }, 5),
            Err(SessionError::Unauthorized)
        );
    }
    fn credential(messages: &[(u64, ServerMessage<TestDocument>)]) -> ResumeCredential {
        messages
            .iter()
            .find_map(|(_, m)| match m {
                ServerMessage::ResumeCredential(c) => Some(c.clone()),
                _ => None,
            })
            .unwrap()
    }
    #[test]
    fn dormant_viewer_keeps_current_role_name_and_expires() {
        let mut a = Authority::<TestDocument>::default();
        let host = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        let ServerMessage::Welcome {
            invitations: Some(invites),
            snapshot,
            ..
        } = &host[0].1
        else {
            panic!()
        };
        let joined = a
            .process(
                2,
                ClientRequest::Join {
                    room: snapshot.room.clone(),
                    invitation: invites.viewer.clone(),
                    name: "Viewer".into(),
                },
                0,
            )
            .unwrap();
        let c = credential(&joined);
        assert_eq!(
            joined
                .iter()
                .filter(|(_, m)| matches!(m, ServerMessage::ResumeCredential(_)))
                .count(),
            1
        );
        assert!(
            joined
                .iter()
                .filter(|(_, m)| matches!(m, ServerMessage::ResumeCredential(_)))
                .all(|(peer, _)| *peer == 2)
        );
        a.process(
            1,
            ClientRequest::SetParticipantRole {
                participant: 2,
                role: Role::Editor,
            },
            1,
        )
        .unwrap();
        a.process(
            2,
            ClientRequest::Rename {
                name: "New name".into(),
            },
            1,
        )
        .unwrap();
        a.disconnect(2, 2);
        let messages = a
            .process(
                3,
                ClientRequest::Resume {
                    credential: c.clone(),
                },
                3,
            )
            .unwrap();
        let ServerMessage::Welcome {
            snapshot,
            invitations,
            ..
        } = &messages
            .iter()
            .find(|(_, m)| matches!(m, ServerMessage::Welcome { .. }))
            .unwrap()
            .1
        else {
            panic!()
        };
        assert!(invitations.is_none());
        assert!(
            snapshot
                .participants
                .iter()
                .any(|p| p.id == 2 && p.role == Role::Editor && p.name == "New name")
        );
        a.disconnect(3, 4);
        assert_eq!(
            a.process(4, ClientRequest::Resume { credential: c }, 1804),
            Err(SessionError::Unauthorized)
        );
    }
    #[test]
    fn invalid_crossroom_and_revoked_credentials_are_rejected() {
        let mut a = Authority::<TestDocument>::default();
        let host = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        let c = credential(&host);
        let mut invalid = c.clone();
        invalid.token.push('x');
        assert_eq!(
            a.process(
                9,
                ClientRequest::Resume {
                    credential: invalid
                },
                1
            ),
            Err(SessionError::Unauthorized)
        );
        let other = a
            .process(
                2,
                ClientRequest::Create {
                    name: "Other".into(),
                },
                0,
            )
            .unwrap();
        let mut invalid = c.clone();
        invalid.room = credential(&other).room;
        assert_eq!(
            a.process(
                9,
                ClientRequest::Resume {
                    credential: invalid
                },
                1
            ),
            Err(SessionError::Unauthorized)
        );
        a.disconnect(1, 2);
        a.expire(1802);
        assert_eq!(
            a.process(9, ClientRequest::Resume { credential: c }, 1802),
            Err(SessionError::Unauthorized)
        );
    }
    #[test]
    fn presenter_resume_restores_only_without_intervening_claim() {
        let mut a = Authority::<TestDocument>::default();
        let host = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        let c = credential(&host);
        a.disconnect(1, 1);
        let restored = a
            .process(
                9,
                ClientRequest::Resume {
                    credential: c.clone(),
                },
                2,
            )
            .unwrap();
        assert!(restored.iter().any(|(_, m)| matches!(m, ServerMessage::Welcome { snapshot, invitations: Some(_), .. } if snapshot.presenter == Some(1))));
        let ServerMessage::Welcome {
            invitations: Some(invites),
            snapshot,
            ..
        } = &host[0].1
        else {
            panic!()
        };
        a.process(
            2,
            ClientRequest::Join {
                room: snapshot.room.clone(),
                invitation: invites.editor.clone(),
                name: "Editor".into(),
            },
            3,
        )
        .unwrap();
        a.disconnect(9, 4);
        a.process(2, ClientRequest::ClaimPresenter, 5).unwrap();
        a.disconnect(2, 6);
        let resumed = a
            .process(10, ClientRequest::Resume { credential: c }, 7)
            .unwrap();
        assert!(resumed.iter().any(|(_, m)| matches!(m, ServerMessage::Welcome { snapshot, .. } if snapshot.presenter.is_none())));
    }
    #[test]
    fn rename_validation_and_invite_join_distinct_identity() {
        let mut a = Authority::<TestDocument>::default();
        let host = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        for name in [" ".to_owned(), "a\nb".to_owned(), "x".repeat(65)] {
            assert_eq!(
                a.process(1, ClientRequest::Rename { name }, 1),
                Err(SessionError::Invalid)
            );
        }
        a.disconnect(1, 2);
        let ServerMessage::Welcome {
            invitations: Some(invites),
            snapshot,
            ..
        } = &host[0].1
        else {
            panic!()
        };
        let joined = a
            .process(
                1,
                ClientRequest::Join {
                    room: snapshot.room.clone(),
                    invitation: invites.editor.clone(),
                    name: "New person".into(),
                },
                3,
            )
            .unwrap();
        assert_ne!(credential(&joined).participant(), 1);
    }
    #[test]
    fn replacing_presenter_rebases_retained_pose_and_sequence() {
        let mut a = Authority::<TestDocument>::default();
        let host = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        let c = credential(&host);
        let camera = CameraSnapshot {
            room: c.room().to_owned(),
            epoch: 1,
            sequence: 50,
            scene_revision: 0,
            pose: CameraPose {
                position: [1., 2., 3.],
                rotation: [0., 0., 0., 1.],
                projection: Projection::Perspective { vertical_fov: 1. },
                focus_distance: 1.,
            },
        };
        a.process(1, ClientRequest::PublishCamera(camera.clone()), 1)
            .unwrap();
        let ServerMessage::Welcome {
            invitations: Some(invites),
            snapshot,
            ..
        } = &host[0].1
        else {
            panic!()
        };
        a.process(
            2,
            ClientRequest::Join {
                room: snapshot.room.clone(),
                invitation: invites.viewer.clone(),
                name: "Follower".into(),
            },
            1,
        )
        .unwrap();
        a.process(
            2,
            ClientRequest::SetFollowing {
                room: snapshot.room.clone(),
                presenter: snapshot.presenter,
                epoch: snapshot.camera_epoch,
                target: snapshot.presenter,
            },
            1,
        )
        .unwrap();
        let resumed = a
            .process(9, ClientRequest::Resume { credential: c }, 2)
            .unwrap();
        let state = resumed
            .iter()
            .find_map(|(_, m)| match m {
                ServerMessage::Welcome { snapshot, .. } => Some(snapshot),
                _ => None,
            })
            .unwrap();
        assert_eq!(state.camera_epoch, 2);
        assert!(
            state.participants.iter().all(|p| p.following.is_none()),
            "replacing presenter invalidates follower confirmation from the old epoch"
        );
        assert_eq!(state.camera.as_ref().unwrap().pose, camera.pose);
        assert_eq!(state.camera.as_ref().unwrap().sequence, 0);
        let mut next = camera;
        next.epoch = 2;
        next.sequence = 1;
        assert!(a.process(9, ClientRequest::PublishCamera(next), 3).is_ok());
    }
    #[cfg(feature = "client")]
    #[test]
    fn transient_resume_rejection_retains_credential_but_unauthorized_clears_it() {
        let mut a = Authority::<TestDocument>::default();
        let messages = a
            .process(
                1,
                ClientRequest::Create {
                    name: "Host".into(),
                },
                0,
            )
            .unwrap();
        let credential = credential(&messages);
        let mut client = ClientSession::<TestDocument>::default();
        client.restore_resume_credential(credential.clone());
        client.status = ConnectionStatus::Connected;
        client
            .send(ClientRequest::Resume {
                credential: credential.clone(),
            })
            .unwrap();
        client.receive(ServerMessage::Rejected(SessionError::Full));
        assert_eq!(client.resume_credential(), Some(&credential));
        assert_eq!(client.error, Some(SessionError::Full));
        client.send(ClientRequest::Resume { credential }).unwrap();
        client.receive(ServerMessage::Rejected(SessionError::Unauthorized));
        assert!(client.resume_credential().is_none());
        assert_eq!(client.error, Some(SessionError::ResumeRejected));
    }
}
