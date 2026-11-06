//! Capacity regressions: dormant state must never deny live participants.
use super::*;
use crate::protocol::tests::TestDocument;

type Messages = Vec<(u64, ServerMessage<TestDocument>)>;

fn create(a: &mut Authority<TestDocument>, peer: u64, now: u64) -> (RoomId, Invitations) {
    let messages = a
        .process(
            peer,
            ClientRequest::Create {
                name: "Host".into(),
            },
            now,
        )
        .unwrap();
    welcome(&messages)
}

fn welcome(messages: &Messages) -> (RoomId, Invitations) {
    messages
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome {
                invitations: Some(invitations),
                snapshot,
                ..
            } => Some((snapshot.room.clone(), invitations.clone())),
            _ => None,
        })
        .expect("owner welcome")
}

fn credential(messages: &Messages) -> crate::ResumeCredential {
    messages
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::ResumeCredential(c) => Some(c.clone()),
            _ => None,
        })
        .expect("private credential")
}

fn join(
    a: &mut Authority<TestDocument>,
    peer: u64,
    room: &RoomId,
    invitation: &Invitation,
    now: u64,
) -> Result<Messages, SessionError> {
    a.process(
        peer,
        ClientRequest::Join {
            room: room.clone(),
            invitation: invitation.clone(),
            name: "Guest".into(),
        },
        now,
    )
}

#[test]
fn reconnecting_viewers_cannot_fill_a_room_with_dormant_records() {
    let mut a = Authority::<TestDocument>::default();
    let (room, invites) = create(&mut a, 1, 0);
    // Every join-then-drop leaves a dormant record; previously the 16th made the room Full.
    for peer in 2..2 + MAX_PARTICIPANTS as u64 * 2 {
        join(&mut a, peer, &room, &invites.viewer, 1).unwrap();
        a.disconnect(peer, 1);
    }
    assert!(join(&mut a, 100, &room, &invites.viewer, 2).is_ok());
    assert!(a.rooms[&room].resumes.len() <= MAX_PARTICIPANTS);
    assert_eq!(a.rooms[&room].state.participants.len(), 2);
}

#[test]
fn eviction_removes_the_longest_dormant_record_first() {
    let mut a = Authority::<TestDocument>::default();
    a.limits.participants = 3;
    let (room, invites) = create(&mut a, 1, 0);
    let oldest = credential(&join(&mut a, 2, &room, &invites.viewer, 0).unwrap());
    let newest = credential(&join(&mut a, 3, &room, &invites.viewer, 0).unwrap());
    a.disconnect(2, 10);
    a.disconnect(3, 20);
    join(&mut a, 4, &room, &invites.viewer, 30).unwrap();
    assert_eq!(
        a.process(5, ClientRequest::Resume { credential: oldest }, 31),
        Err(SessionError::Unauthorized)
    );
    assert!(
        a.process(6, ClientRequest::Resume { credential: newest }, 31)
            .is_ok()
    );
}

#[test]
fn live_participants_still_fill_a_room() {
    let mut a = Authority::<TestDocument>::default();
    a.limits.participants = 2;
    let (room, invites) = create(&mut a, 1, 0);
    join(&mut a, 2, &room, &invites.viewer, 0).unwrap();
    assert_eq!(
        join(&mut a, 3, &room, &invites.viewer, 0),
        Err(SessionError::Full)
    );
}

#[test]
fn create_then_leave_churn_cannot_exhaust_rooms() {
    let mut a = Authority::<TestDocument>::default();
    a.limits.rooms = 2;
    // Left rooms stay joinable by invitation, but previously also blocked Create.
    for round in 0..10 {
        create(&mut a, 1, round);
        a.process(1, ClientRequest::Leave, round).unwrap();
    }
    assert_eq!(a.rooms.len(), 2);
    let (live, _) = create(&mut a, 2, 10);
    assert!(a.rooms.contains_key(&live));
}

#[test]
fn abandoned_rooms_yield_to_new_rooms_but_live_rooms_do_not() {
    let mut a = Authority::<TestDocument>::default();
    a.limits.rooms = 2;
    let (first, _) = create(&mut a, 1, 0);
    let (second, _) = create(&mut a, 2, 0);
    a.disconnect(1, 5);
    a.disconnect(2, 6);
    let (third, _) = create(&mut a, 3, 7);
    assert!(!a.rooms.contains_key(&first), "longest-empty room evicted");
    assert!(a.rooms.contains_key(&second));
    create(&mut a, 4, 8);
    assert_eq!(
        a.process(
            5,
            ClientRequest::Create {
                name: "Late".into()
            },
            9
        ),
        Err(SessionError::Full)
    );
    assert!(a.rooms.contains_key(&third));
}

#[test]
fn tampered_resume_token_is_rejected() {
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
    let mut forged = credential(&messages);
    a.disconnect(1, 1);
    let last = forged.token.pop().unwrap();
    forged.token.push(if last == '0' { '1' } else { '0' });
    assert_eq!(
        a.process(2, ClientRequest::Resume { credential: forged }, 2),
        Err(SessionError::Unauthorized)
    );
}

#[test]
fn same_secret_matches_only_identical_tokens() {
    assert!(same_secret("abc", "abc"));
    assert!(same_secret("", ""));
    assert!(!same_secret("abc", "abd"));
    assert!(!same_secret("abc", "ab"));
    assert!(!same_secret("ab", "abc"));
}
