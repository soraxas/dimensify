use crate::ResumeCredential;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_PARTICIPANTS: usize = 16;
pub const MAX_TEXT: usize = 2048;
pub type ParticipantId = u64;
pub type RoomId = String;
/// Unique for this room, independent of requester and transport identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenterRequest {
    pub participant: ParticipantId,
    pub id: u64,
}

// Do not derive Debug: invitations are credentials, including inside envelopes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitation(#[serde(deserialize_with = "bounded_text")] pub String);
impl fmt::Debug for Invitation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Invitation([redacted])")
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invitations {
    pub viewer: Invitation,
    pub editor: Invitation,
    pub codes: Option<crate::InviteCodes>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Viewer,
    Editor,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    pub id: ParticipantId,
    #[serde(deserialize_with = "bounded_text")]
    pub name: String,
    pub role: Role,
    /// Confirmed follow target, independent of presentation authority and cursor sharing.
    pub following: Option<ParticipantId>,
}
/// Application-selected state. Default must be valid; validation runs on the server
/// before acceptance and on clients before applying snapshots. Bound collections
/// while deserializing untrusted state, rather than only after allocation.
/// SCHEMA identifies the complete application wire contract; change it for
/// incompatible payload layouts. One schema is installed per app/endpoint.
pub trait SharedDocument:
    Clone
    + fmt::Debug
    + Default
    + PartialEq
    + Serialize
    + serde::de::DeserializeOwned
    + Send
    + Sync
    + 'static
{
    const SCHEMA: &'static str;
    fn validate(&self) -> bool;
    fn registrations() -> crate::RegistrationSet<Self> {
        crate::RegistrationSet::default()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Document<D> {
    pub revision: u64,
    pub data: D,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    Perspective { vertical_fov: f32 },
    Orthographic { vertical_size: f32 },
}
/// Canonical repository coordinates: right-handed, +Z up; XYZW quaternion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraPose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub projection: Projection,
    pub focus_distance: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraSnapshot {
    #[serde(deserialize_with = "bounded_text")]
    pub room: RoomId,
    pub epoch: u64,
    pub sequence: u64,
    pub scene_revision: u64,
    pub pose: CameraPose,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomSnapshot<D> {
    #[serde(deserialize_with = "bounded_text")]
    pub room: RoomId,
    /// Changes only for room/document policy, not camera packets.
    pub version: u64,
    pub document: Document<D>,
    #[serde(deserialize_with = "bounded_participants")]
    pub participants: Vec<Participant>,
    pub owner: ParticipantId,
    pub presenter: Option<ParticipantId>,
    #[serde(deserialize_with = "bounded_requests")]
    pub presenter_requests: Vec<PresenterRequest>,
    pub camera_epoch: u64,
    pub forced_follow: bool,
    pub camera: Option<CameraSnapshot>,
}
impl<D> RoomSnapshot<D> {
    /// Resolve a participant's view through a bounded follow chain; reject cycles or missing peers.
    pub fn view_source(&self, participant: ParticipantId) -> Option<ParticipantId> {
        let mut id = participant;
        for _ in 0..self.participants.len() {
            let peer = self.participants.iter().find(|p| p.id == id)?;
            match peer.following {
                Some(next) => id = next,
                None => return Some(id),
            }
        }
        None
    }

    /// Check a proposed local follow choice without changing anyone else's view or permissions.
    pub fn can_follow(&self, you: ParticipantId, target: Option<ParticipantId>) -> bool {
        if !self.participants.iter().any(|p| p.id == you) {
            return false;
        }
        if self.forced_follow
            && target != self.presenter
            && !(self.presenter == Some(you) && target.is_none())
        {
            return false;
        }
        let Some(mut id) = target else { return true };
        for _ in 0..self.participants.len() {
            if id == you {
                return false;
            }
            let Some(peer) = self.participants.iter().find(|p| p.id == id) else {
                return false;
            };
            match peer.following {
                Some(next) => id = next,
                None => return true,
            }
        }
        false
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientRequest<D> {
    JoinWithCode {
        code: crate::InviteCode,
        #[serde(deserialize_with = "bounded_text")]
        name: String,
    },
    RenewInviteCodes,
    Resume {
        credential: ResumeCredential,
    },
    Rename {
        #[serde(deserialize_with = "bounded_text")]
        name: String,
    },
    Probe {
        nonce: u64,
    },
    Heartbeat,
    Create {
        #[serde(deserialize_with = "bounded_text")]
        name: String,
    },
    Join {
        #[serde(deserialize_with = "bounded_text")]
        room: RoomId,
        invitation: Invitation,
        #[serde(deserialize_with = "bounded_text")]
        name: String,
    },
    SubmitDocument {
        id: EditId,
        expected_revision: u64,
        data: D,
    },
    SetDocument {
        expected_revision: u64,
        data: D,
    },
    RequestPresenter,
    GrantPresenter {
        request: PresenterRequest,
    },
    DismissPresenter {
        request: PresenterRequest,
    },
    ClaimPresenter,
    SetForcedFollow(bool),
    /// Bring current participants to the presenter once, without locking navigation.
    Spotlight,
    /// Transfer presentation authority without requiring a prior control request.
    MakePresenter {
        participant: ParticipantId,
    },
    /// Authenticated sender's selected follow target in the current room context.
    SetFollowing {
        #[serde(deserialize_with = "bounded_text")]
        room: RoomId,
        presenter: Option<ParticipantId>,
        epoch: u64,
        target: Option<ParticipantId>,
    },
    /// Every participant may publish their own rendered view.
    PublishView(CameraSnapshot),
    PublishCamera(CameraSnapshot),
    Leave,
    SetCursorSubscription(crate::CursorSubscription),
    PublishCursor(crate::CursorSample),
    SetParticipantRole {
        participant: ParticipantId,
        role: Role,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionError {
    Replaced,
    ResumeRejected,
    Unauthorized,
    Invalid,
    Conflict,
    Stale,
    Full,
    AlreadyJoined,
    NotJoined,
    Transport,
    QueueFull,
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SessionError {}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage<D> {
    Invitations(Invitations),
    Probe {
        nonce: u64,
    },
    PresenterRequested(PresenterRequest),
    PresenterDecisionRejected {
        request: PresenterRequest,
        error: SessionError,
    },
    PresenterDecision {
        request: PresenterRequest,
        granted: bool,
    },
    DocumentEditOutcome(DocumentEditOutcome),
    ResumeCredential(ResumeCredential),
    Heartbeat,
    Replaced,
    Welcome {
        you: ParticipantId,
        invitations: Option<Invitations>,
        snapshot: RoomSnapshot<D>,
    },
    Snapshot(RoomSnapshot<D>),
    Camera(CameraSnapshot),
    /// A one-shot navigation request scoped to the current presentation context.
    Spotlight {
        room: RoomId,
        presenter: ParticipantId,
        epoch: u64,
    },
    /// Sender identity is supplied by the authority, never by the publishing client.
    View {
        participant: ParticipantId,
        camera: CameraSnapshot,
    },
    Rejected(SessionError),
    Left,
    Cursor(crate::RemoteCursor),
    CursorSubscription(crate::CursorSubscription),
}

/// Check that a camera pose is finite and has a valid rotation and projection.
pub fn valid_pose(pose: &CameraPose) -> bool {
    bounded_vector(&pose.position, 1e7)
        && valid_rotation(&pose.rotation)
        && pose.focus_distance.is_finite()
        && (0.001..=1e7).contains(&pose.focus_distance)
        && match pose.projection {
            Projection::Perspective { vertical_fov } => {
                vertical_fov.is_finite() && (0.01..=165f32.to_radians()).contains(&vertical_fov)
            }
            Projection::Orthographic { vertical_size } => {
                vertical_size.is_finite() && (0.001..=1e7).contains(&vertical_size)
            }
        }
}
fn bounded_vector(v: &[f32], limit: f32) -> bool {
    v.iter().all(|x| x.is_finite() && x.abs() <= limit)
}
fn valid_rotation(v: &[f32; 4]) -> bool {
    bounded_vector(v, 1.001) && (v.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 0.001
}
/// Deserialize text within the protocol payload limit.
pub fn bounded_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = String;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bounded UTF-8 text")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<String, E> {
            if v.len() > MAX_TEXT {
                Err(E::custom("text too long"))
            } else {
                Ok(v.to_owned())
            }
        }
    }
    d.deserialize_str(Visitor)
}
fn bounded_participants<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Participant>, D::Error> {
    bounded_list::<D, Participant>(d)
}
fn bounded_requests<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PresenterRequest>, D::Error> {
    bounded_list::<D, PresenterRequest>(d)
}
fn bounded_list<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("at most sixteen participants")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_PARTICIPANTS) {
                return Err(serde::de::Error::custom("too many participants"));
            }
            let mut items = Vec::new();
            while let Some(item) = seq.next_element()? {
                if items.len() == MAX_PARTICIPANTS {
                    return Err(serde::de::Error::custom("too many participants"));
                }
                items.push(item);
            }
            Ok(items)
        }
    }
    d.deserialize_seq(Visitor(std::marker::PhantomData))
}

/// Validate an authoritative snapshot before an adapter can act on it.
pub fn valid_snapshot<D: SharedDocument>(state: &RoomSnapshot<D>) -> bool {
    !state.room.is_empty()
        && state.room.len() <= 64
        && state.document.data.validate()
        && !state.participants.is_empty()
        && state.participants.len() <= MAX_PARTICIPANTS
        && state.participants.iter().enumerate().all(|(i, p)| {
            !p.name.trim().is_empty()
                && p.name.len() <= 64
                && !p.name.chars().any(char::is_control)
                && !state.participants[..i].iter().any(|old| old.id == p.id)
                && state.view_source(p.id).is_some()
        })
        && state
            .presenter
            .is_none_or(|id| state.participants.iter().any(|p| p.id == id))
        && (!state.forced_follow || state.presenter.is_some())
        && state.presenter_requests.len() <= MAX_PARTICIPANTS
        && state.presenter_requests.iter().enumerate().all(|(i, id)| {
            state.participants.iter().any(|p| p.id == id.participant)
                && !state.presenter_requests[..i]
                    .iter()
                    .any(|old| old.id == id.id || old.participant == id.participant)
        })
        && state.camera.as_ref().is_none_or(|c| {
            state.presenter.is_some()
                && c.room == state.room
                && c.epoch == state.camera_epoch
                && c.scene_revision == state.document.revision
                && valid_pose(&c.pose)
        })
}

/// Display names are stored trimmed, with a 64-byte UTF-8 bound and no controls.
pub fn valid_display_name(name: &str) -> bool {
    !name.trim().is_empty() && name.trim().len() <= 64 && !name.chars().any(char::is_control)
}

/// Unique within one client transport session; outcomes never acknowledge controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentEditOutcome {
    Accepted { id: EditId, revision: u64 },
    Rejected { id: EditId, error: SessionError },
}
impl DocumentEditOutcome {
    /// Return the edit correlation identifier for this outcome.
    pub fn id(self) -> EditId {
        match self {
            Self::Accepted { id, .. } | Self::Rejected { id, .. } => id,
        }
    }
}
