//! Server-scoped, role-specific invitations, independent of browser addresses.
use crate::{SessionError, bounded_text};
use serde::{Deserialize, Serialize};
use std::fmt;

const ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
pub const INVITE_CODE_LIFETIME_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteCode(#[serde(deserialize_with = "bounded_text")] String);
impl fmt::Debug for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InviteCode([redacted])")
    }
}
impl InviteCode {
    /// Normalize and validate a bounded human-readable invitation code.
    pub fn parse(value: &str) -> Result<Self, SessionError> {
        if value.len() > 64 {
            return Err(SessionError::Invalid);
        }
        let value: String = value
            .bytes()
            .filter(|b| *b != b'-' && !b.is_ascii_whitespace())
            .map(|b| char::from(b.to_ascii_uppercase()))
            .collect();
        if value.len() != 12 || !value.bytes().all(|b| ALPHABET.contains(&b)) {
            return Err(SessionError::Invalid);
        }
        Ok(Self(value))
    }
    /// Explicit credential disclosure for user-facing copy/display controls only.
    pub fn display_code(&self) -> String {
        self.0
            .as_bytes()
            .chunks(4)
            .map(|chunk| String::from_utf8_lossy(chunk))
            .collect::<Vec<_>>()
            .join("-")
    }
    #[cfg(feature = "server")]
    fn generate() -> Result<Self, SessionError> {
        let mut bytes = [0u8; 12];
        getrandom::fill(&mut bytes).map_err(|_| SessionError::Transport)?;
        Ok(Self(
            bytes
                .iter()
                .map(|b| char::from(ALPHABET[(b & 31) as usize]))
                .collect(),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteCodes {
    pub viewer: InviteCode,
    pub editor: InviteCode,
}

#[cfg(feature = "server")]
impl<D: crate::SharedDocument> crate::authority::Authority<D> {
    /// Generate distinct invitation codes not already assigned to a live room.
    pub(crate) fn new_codes(&self) -> Result<InviteCodes, SessionError> {
        let generate = |other: Option<&InviteCode>| {
            for _ in 0..16 {
                let code = InviteCode::generate()?;
                if other != Some(&code)
                    && !self.rooms.values().any(|room| {
                        room.invitations
                            .codes
                            .as_ref()
                            .is_some_and(|c| c.viewer == code || c.editor == code)
                    })
                {
                    return Ok(code);
                }
            }
            Err(SessionError::Transport)
        };
        let viewer = generate(None)?;
        let editor = generate(Some(&viewer))?;
        Ok(InviteCodes { viewer, editor })
    }
    /// Resolve an unexpired code to its room and role-specific invitation.
    pub(crate) fn resolve_code(
        &self,
        code: &InviteCode,
        now: u64,
    ) -> Result<(String, crate::Invitation), SessionError> {
        let code = InviteCode::parse(&code.0)?;
        self.rooms
            .iter()
            .find_map(|(id, room)| {
                if now >= room.codes_expire_at {
                    return None;
                }
                let codes = room.invitations.codes.as_ref()?;
                let invitation = if code == codes.viewer {
                    &room.invitations.viewer
                } else if code == codes.editor {
                    &room.invitations.editor
                } else {
                    return None;
                };
                Some((id.clone(), invitation.clone()))
            })
            .ok_or(SessionError::Unauthorized)
    }
    /// Rotate a room owner's invitation codes while retaining existing members.
    pub(crate) fn renew_codes(
        &mut self,
        peer: u64,
        now: u64,
    ) -> Result<Vec<(u64, crate::ServerMessage<D>)>, SessionError> {
        let id = self.members.get(&peer).ok_or(SessionError::NotJoined)?;
        let room = self.rooms.get(id).ok_or(SessionError::NotJoined)?;
        if room.state.owner != peer {
            return Err(SessionError::Unauthorized);
        }
        let codes = self.new_codes()?;
        let room = self.rooms.get_mut(id).ok_or(SessionError::NotJoined)?;
        room.invitations.codes = Some(codes);
        room.codes_expire_at = now.saturating_add(INVITE_CODE_LIFETIME_SECONDS);
        Ok(vec![(
            peer,
            crate::ServerMessage::Invitations(room.invitations.clone()),
        )])
    }
}

#[cfg(all(test, feature = "server", feature = "client"))]
mod tests {
    use super::*;
    use crate::{authority::Authority, protocol::tests::TestDocument, *};

    fn create(
        a: &mut Authority<TestDocument>,
        peer: u64,
    ) -> (InviteCodes, RoomSnapshot<TestDocument>) {
        let messages = a
            .process(
                peer,
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
        } = &messages[0].1
        else {
            panic!("welcome")
        };
        (invites.codes.clone().unwrap(), snapshot.clone())
    }
    fn join(code: InviteCode) -> ClientRequest<TestDocument> {
        ClientRequest::JoinWithCode {
            code,
            name: "Guest".into(),
        }
    }
    #[test]
    fn codes_join_with_server_assigned_roles_and_private_resume_credentials() {
        let mut a = Authority::<TestDocument>::default();
        let (codes, room) = create(&mut a, 10);
        assert_ne!(codes.viewer, codes.editor);
        for (peer, code, role) in [
            (20, codes.viewer, Role::Viewer),
            (30, codes.editor, Role::Editor),
        ] {
            let code = InviteCode::parse(&code.display_code().to_ascii_lowercase()).unwrap();
            let messages = a.process(peer, join(code), 1).unwrap();
            let mut client = ClientSession::<TestDocument>::default();
            for (_, message) in messages.iter().filter(|(id, _)| *id == peer) {
                client.receive(message.clone());
            }
            assert_eq!(client.status, ConnectionStatus::Joined);
            let snapshot = client.snapshot.as_ref().unwrap();
            assert_eq!(snapshot.room, room.room);
            assert_eq!(
                snapshot
                    .participants
                    .iter()
                    .find(|p| Some(p.id) == client.you)
                    .unwrap()
                    .role,
                role
            );
            assert!(client.invitations.is_none());
            let credential = messages
                .iter()
                .find_map(|(id, m)| match m {
                    ServerMessage::ResumeCredential(c) => {
                        assert_eq!(*id, peer);
                        Some(c.clone())
                    }
                    _ => None,
                })
                .unwrap();
            a.disconnect(peer, 2);
            assert!(
                a.process(peer + 1, ClientRequest::Resume { credential }, 3)
                    .is_ok()
            );
        }
    }
    #[test]
    fn renewal_is_owner_only_revokes_codes_and_preserves_members() {
        let mut a = Authority::<TestDocument>::default();
        let (old, room) = create(&mut a, 10);
        a.process(20, join(old.editor.clone()), 1).unwrap();
        assert_eq!(
            a.process(20, ClientRequest::RenewInviteCodes, 2),
            Err(SessionError::Unauthorized)
        );
        let messages = a.process(10, ClientRequest::RenewInviteCodes, 3).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, 10);
        let ServerMessage::Invitations(invites) = &messages[0].1 else {
            panic!("private codes")
        };
        let new = invites.codes.clone().unwrap();
        assert_ne!(new, old);
        assert_eq!(
            a.process(30, join(old.viewer), 4),
            Err(SessionError::Unauthorized)
        );
        assert_eq!(
            a.process(30, join(old.editor), 4),
            Err(SessionError::Unauthorized)
        );
        assert!(a.process(30, join(new.viewer), 4).is_ok());
        assert!(a.is_joined(20));
        assert_eq!(a.rooms[&room.room].state.participants.len(), 3);
    }
    #[test]
    fn codes_expire_and_are_scoped_to_the_issuing_server_and_room_lifetime() {
        let mut a = Authority::<TestDocument>::default();
        let (codes, _) = create(&mut a, 10);
        let mut other = Authority::<TestDocument>::default();
        assert_eq!(
            other.process(20, join(codes.viewer.clone()), 1),
            Err(SessionError::Unauthorized)
        );
        assert_eq!(
            a.process(20, join(codes.viewer.clone()), INVITE_CODE_LIFETIME_SECONDS),
            Err(SessionError::Unauthorized)
        );
        a.process(
            10,
            ClientRequest::RenewInviteCodes,
            INVITE_CODE_LIFETIME_SECONDS,
        )
        .unwrap();
        let code = a
            .rooms
            .values()
            .next()
            .unwrap()
            .invitations
            .codes
            .as_ref()
            .unwrap()
            .viewer
            .clone();
        a.disconnect(10, INVITE_CODE_LIFETIME_SECONDS);
        assert_eq!(
            a.process(20, join(code), INVITE_CODE_LIFETIME_SECONDS + 1800),
            Err(SessionError::Unauthorized)
        );
        assert!(a.rooms.is_empty());
    }
    #[test]
    fn code_input_is_bounded_normalized_and_redacted() {
        let code = InviteCode::parse(" abcd-efgh-jkmn ").unwrap();
        assert_eq!(code.display_code(), "ABCD-EFGH-JKMN");
        assert!(!format!("{:?}", join(code)).contains("ABCD"));
        for value in ["", "1234-5678-9012", "ABCD-EFGH-JKM!", &"A".repeat(65)] {
            assert!(InviteCode::parse(value).is_err());
        }
    }
}
