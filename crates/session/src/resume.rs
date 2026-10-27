//! Private bearer credential for one stable room participant.
use crate::{ParticipantId, RoomId, bounded_text};
use serde::{Deserialize, Serialize};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeCredential {
    #[serde(deserialize_with = "bounded_text")]
    pub(crate) room: RoomId,
    pub(crate) participant: ParticipantId,
    #[serde(deserialize_with = "bounded_text")]
    pub(crate) token: String,
}
impl ResumeCredential {
    /// Return the room associated with this private resume credential.
    pub fn room(&self) -> &str {
        &self.room
    }
    /// Return the stable participant identity associated with this credential.
    pub fn participant(&self) -> ParticipantId {
        self.participant
    }
}
impl std::fmt::Debug for ResumeCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResumeCredential([redacted])")
    }
}
#[cfg(feature = "server")]
pub(crate) struct ResumeRecord {
    pub credential: ResumeCredential,
    pub participant: crate::Participant,
    pub dormant_since: Option<u64>,
    pub presenter_epoch: Option<u64>,
    pub camera: Option<crate::CameraSnapshot>,
}

#[cfg(feature = "server")]
use crate::{
    authority::{Authority, broadcast, token},
    *,
};
#[cfg(feature = "server")]
impl<D: SharedDocument> Authority<D> {
    fn route(&self, messages: Vec<(u64, ServerMessage<D>)>) -> Vec<(u64, ServerMessage<D>)> {
        messages
            .into_iter()
            .filter_map(|(id, message)| {
                self.connections
                    .iter()
                    .find(|(_, participant)| **participant == id)
                    .map(|(peer, _)| (*peer, message))
            })
            .collect()
    }
    /// Detach a transport while retaining eligible private resume state.
    pub fn disconnect(&mut self, peer: u64, now: u64) -> Vec<(u64, ServerMessage<D>)> {
        let Some(id) = self.connections.remove(&peer) else {
            return vec![];
        };
        let messages = self.disconnect_identity(id, now);
        self.route(messages)
    }
    /// Process a request using a transport identity and authority time in seconds.
    pub fn process(
        &mut self,
        peer: u64,
        request: ClientRequest<D>,
        now: u64,
    ) -> Result<Vec<(u64, ServerMessage<D>)>, SessionError> {
        self.expire(now);
        if let ClientRequest::Probe { nonce } = request {
            return Ok(vec![(peer, ServerMessage::Probe { nonce })]);
        }
        if let ClientRequest::SubmitDocument {
            id,
            expected_revision,
            data,
        } = request
        {
            let (mut messages, outcome) = match self.process(
                peer,
                ClientRequest::SetDocument {
                    expected_revision,
                    data,
                },
                now,
            ) {
                Ok(messages) => (
                    messages,
                    DocumentEditOutcome::Accepted {
                        id,
                        revision: expected_revision + 1,
                    },
                ),
                Err(error) => (vec![], DocumentEditOutcome::Rejected { id, error }),
            };
            messages.push((peer, ServerMessage::DocumentEditOutcome(outcome)));
            return Ok(messages);
        }
        if let ClientRequest::Resume { credential } = request {
            if self.is_joined(peer) {
                return Err(SessionError::AlreadyJoined);
            }
            let room = self
                .rooms
                .get_mut(&credential.room)
                .ok_or(SessionError::Unauthorized)?;
            let record = room
                .resumes
                .get(&credential.participant)
                .filter(|r| r.credential == credential)
                .ok_or(SessionError::Unauthorized)?;
            let participant = room
                .state
                .participants
                .iter()
                .find(|p| p.id == credential.participant)
                .cloned()
                .unwrap_or_else(|| record.participant.clone());
            let restore = record.presenter_epoch == Some(room.state.camera_epoch)
                && room.state.presenter.is_none();
            let replacing_presenter = room.state.presenter == Some(credential.participant);
            if !room
                .state
                .participants
                .iter()
                .any(|p| p.id == credential.participant)
                && room.state.participants.len() >= self.limits.participants.min(MAX_PARTICIPANTS)
            {
                return Err(SessionError::Full);
            }
            let retained_camera = if replacing_presenter {
                room.state.camera.clone()
            } else {
                record.camera.clone()
            };
            let old = self
                .connections
                .iter()
                .find(|(_, id)| **id == credential.participant)
                .map(|(p, _)| *p);
            if let Some(old) = old {
                self.connections.remove(&old);
            }
            self.connections.insert(peer, credential.participant);
            self.members
                .insert(credential.participant, credential.room.clone());
            if !room
                .state
                .participants
                .iter()
                .any(|p| p.id == participant.id)
            {
                room.state.participants.push(participant);
            }
            if replacing_presenter {
                room.state.camera_epoch += 1;
            }
            if restore || replacing_presenter {
                room.state.presenter = Some(credential.participant);
                room.state.camera = retained_camera
                    .filter(|c| c.scene_revision == room.state.document.revision)
                    .map(|mut c| {
                        c.epoch = room.state.camera_epoch;
                        c.sequence = 0;
                        c
                    });
            }
            // A new transport must confirm actual follow state again. Replacing
            // a presenter also invalidates every follower's previous epoch.
            for participant in &mut room.state.participants {
                if restore || replacing_presenter || participant.id == credential.participant {
                    participant.following = None;
                }
            }
            if let Some(record) = room.resumes.get_mut(&credential.participant) {
                record.dormant_since = None;
            }
            room.empty_since = None;
            room.state.version += 1;
            room.cursors.remove(credential.participant);
            room.views.remove(&credential.participant);
            let mut messages = broadcast(room);
            messages.retain(|(id, _)| *id != credential.participant);
            messages.push((
                credential.participant,
                ServerMessage::Welcome {
                    you: credential.participant,
                    invitations: (room.state.owner == credential.participant)
                        .then(|| room.invitations.clone()),
                    snapshot: room.state.clone(),
                },
            ));
            messages.push((
                credential.participant,
                ServerMessage::ResumeCredential(credential),
            ));
            let mut routed = self.route(messages);
            if let Some(old) = old {
                routed.push((old, ServerMessage::Replaced));
            }
            return Ok(routed);
        }
        let joining = matches!(
            request,
            ClientRequest::Create { .. }
                | ClientRequest::Join { .. }
                | ClientRequest::JoinWithCode { .. }
        );
        if joining && self.is_joined(peer) {
            return Err(SessionError::AlreadyJoined);
        }
        let identity = if joining {
            let id = self.next_identity;
            self.next_identity = self
                .next_identity
                .checked_add(1)
                .ok_or(SessionError::Full)?;
            id
        } else {
            *self.connections.get(&peer).ok_or(SessionError::NotJoined)?
        };
        let leaving = matches!(request, ClientRequest::Leave);
        let credential_token = if joining { Some(token()?) } else { None };
        let mut messages = self.process_identity(identity, request, now)?;
        if joining {
            self.connections.insert(peer, identity);
            let id = self
                .members
                .get(&identity)
                .ok_or(SessionError::NotJoined)?
                .clone();
            let room = self.rooms.get_mut(&id).ok_or(SessionError::NotJoined)?;
            let participant = room
                .state
                .participants
                .iter()
                .find(|p| p.id == identity)
                .cloned()
                .ok_or(SessionError::NotJoined)?;
            let credential = ResumeCredential {
                room: id,
                participant: identity,
                token: credential_token.ok_or(SessionError::Transport)?,
            };
            room.resumes.insert(
                identity,
                crate::resume::ResumeRecord {
                    credential: credential.clone(),
                    participant,
                    dormant_since: None,
                    presenter_epoch: None,
                    camera: None,
                },
            );
            messages.push((identity, ServerMessage::ResumeCredential(credential)));
        }
        let routed = self.route(messages);
        if leaving {
            self.connections.remove(&peer);
        }
        Ok(routed)
    }
}
