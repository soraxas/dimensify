#![cfg(feature = "server")]
use dimensify_session::*;

// Follow presence is reusable without an application camera registration.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct SharedState;
impl SharedDocument for SharedState {
    const SCHEMA: &'static str = "follow-presence-test.v1";
    fn validate(&self) -> bool {
        true
    }
}
type Authority = authority::Authority<SharedState>;
type RoomSnapshot = dimensify_session::RoomSnapshot<SharedState>;
type ClientRequest = dimensify_session::ClientRequest<SharedState>;
type ServerMessage = dimensify_session::ServerMessage<SharedState>;
#[cfg(feature = "client")]
type ClientSession = dimensify_session::ClientSession<SharedState>;

fn created(a: &mut Authority, peer: u64) -> (Invitations, RoomSnapshot) {
    let r = a
        .process(
            peer,
            ClientRequest::Create {
                name: "host".into(),
            },
            0,
        )
        .unwrap();
    let ServerMessage::Welcome {
        invitations: Some(i),
        snapshot: s,
        ..
    } = r[0].1.clone()
    else {
        panic!()
    };
    (i, s)
}
#[test]
fn follow_presence_starts_unconfirmed() {
    let mut authority = Authority::default();
    let (_, state) = created(&mut authority, 1);
    let participant = serde_json::to_value(&state.participants[0]).unwrap();
    assert_eq!(participant["following"], serde_json::Value::Null);
}

fn follow_request(state: &RoomSnapshot, following: bool) -> ClientRequest {
    ClientRequest::SetFollowing {
        room: state.room.clone(),
        presenter: state.presenter,
        epoch: state.camera_epoch,
        target: if following { state.presenter } else { None },
    }
}
fn snapshot(messages: &[(u64, ServerMessage)]) -> RoomSnapshot {
    messages
        .iter()
        .find_map(|(_, message)| match message {
            ServerMessage::Snapshot(state)
            | ServerMessage::Welcome {
                snapshot: state, ..
            } => Some(state.clone()),
            _ => None,
        })
        .unwrap()
}
fn follow_room() -> (Authority, RoomSnapshot, ResumeCredential) {
    let mut a = Authority::default();
    let (invites, state) = created(&mut a, 1);
    let joined = a
        .process(
            2,
            ClientRequest::Join {
                room: state.room,
                invitation: invites.editor,
                name: "viewer".into(),
            },
            0,
        )
        .unwrap();
    let credential = joined
        .iter()
        .find_map(|(_, message)| match message {
            ServerMessage::ResumeCredential(credential) => Some(credential.clone()),
            _ => None,
        })
        .unwrap();
    (a, snapshot(&joined), credential)
}
fn follows(state: &RoomSnapshot, id: u64) -> bool {
    state
        .participants
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .following
        .is_some()
}
#[test]
fn follow_presence_authenticates_sender_rejects_stale_context_and_deduplicates() {
    let (mut a, state, _) = follow_room();
    assert_eq!(
        a.process(9, follow_request(&state, true), 0),
        Err(SessionError::NotJoined)
    );
    assert_eq!(
        a.process(1, follow_request(&state, true), 0),
        Err(SessionError::Unauthorized)
    );
    let mut stale = state.clone();
    stale.camera_epoch += 1;
    assert_eq!(
        a.process(2, follow_request(&stale, true), 0),
        Err(SessionError::Stale)
    );
    stale = state.clone();
    stale.presenter = Some(2);
    assert_eq!(
        a.process(2, follow_request(&stale, true), 0),
        Err(SessionError::Stale)
    );
    stale = state.clone();
    stale.room = "other-room".into();
    assert_eq!(
        a.process(2, follow_request(&stale, true), 0),
        Err(SessionError::Stale)
    );
    // No cursor subscription is installed: reliable presence remains available.
    let mut spoofed = serde_json::to_value(follow_request(&state, true)).unwrap();
    spoofed["SetFollowing"]["participant"] = serde_json::json!(1);
    let replies = a
        .process(2, serde_json::from_value(spoofed).unwrap(), 0)
        .unwrap();
    assert_eq!(replies.len(), 2);
    let following = snapshot(&replies);
    assert!(!follows(&following, 1));
    assert!(follows(&following, 2));
    assert_eq!(following.version, state.version + 1);
    assert_eq!(following.document, state.document);
    assert!(
        a.process(2, follow_request(&state, true), 0)
            .unwrap()
            .is_empty()
    );
    let stopped = snapshot(&a.process(2, follow_request(&state, false), 0).unwrap());
    assert!(!follows(&stopped, 2));
    assert_eq!(stopped.version, following.version + 1);
}
#[test]
fn forced_follow_waits_for_actual_confirmation_and_cannot_be_declined() {
    let (mut a, state, _) = follow_room();
    let forced = snapshot(
        &a.process(1, ClientRequest::SetForcedFollow(true), 0)
            .unwrap(),
    );
    assert!(
        !follows(&forced, 2),
        "policy must not claim actual follow presence"
    );
    assert_eq!(
        a.process(2, follow_request(&state, false), 0),
        Err(SessionError::Unauthorized)
    );
    let following = snapshot(&a.process(2, follow_request(&state, true), 0).unwrap());
    assert!(
        a.process(1, ClientRequest::SetForcedFollow(true), 0)
            .unwrap()
            .is_empty(),
        "reapplying the lock must preserve confirmed following"
    );
    assert!(follows(&following, 2));
    a.process(1, ClientRequest::SetForcedFollow(false), 0)
        .unwrap();
    let stopped = snapshot(&a.process(2, follow_request(&state, false), 0).unwrap());
    assert!(!follows(&stopped, 2));
}
#[test]
fn handover_and_presenter_disconnect_clear_previous_follow_presence() {
    let (mut a, state, _) = follow_room();
    a.process(2, follow_request(&state, true), 0).unwrap();
    let request = a
        .process(2, ClientRequest::RequestPresenter, 0)
        .unwrap()
        .into_iter()
        .find_map(|(_, m)| match m {
            ServerMessage::PresenterRequested(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let handover = snapshot(
        &a.process(1, ClientRequest::GrantPresenter { request }, 0)
            .unwrap(),
    );
    assert!(!follows(&handover, 1));
    assert!(!follows(&handover, 2));
    assert_eq!(
        a.process(1, follow_request(&state, true), 0),
        Err(SessionError::Stale)
    );
    a.process(1, follow_request(&handover, true), 0).unwrap();
    let disconnected = snapshot(&a.disconnect(2, 0));
    assert_eq!(disconnected.presenter, None);
    assert!(!follows(&disconnected, 1));
    assert_eq!(disconnected.participants.len(), 1);
    a.process(1, ClientRequest::ClaimPresenter, 0).unwrap();
}
#[test]
fn resuming_follower_requires_fresh_confirmation_even_when_replacing_transport() {
    let (mut a, state, credential) = follow_room();
    a.process(2, follow_request(&state, true), 0).unwrap();
    let replaced = snapshot(
        &a.process(
            3,
            ClientRequest::Resume {
                credential: credential.clone(),
            },
            0,
        )
        .unwrap(),
    );
    assert!(!follows(&replaced, 2));
    assert_eq!(
        a.process(2, follow_request(&state, true), 0),
        Err(SessionError::NotJoined)
    );
    a.process(3, follow_request(&replaced, true), 0).unwrap();
    let disconnected = snapshot(&a.disconnect(3, 0));
    assert_eq!(disconnected.participants.len(), 1);
    let resumed = snapshot(
        &a.process(4, ClientRequest::Resume { credential }, 0)
            .unwrap(),
    );
    assert!(!follows(&resumed, 2));
}

#[cfg(feature = "client")]
#[test]
fn client_follow_presence_is_idempotent_retries_queue_full_and_republishes_after_handover() {
    let (_, state, _) = follow_room();
    let mut client = ClientSession::default();
    client.receive(ServerMessage::Welcome {
        you: 2,
        invitations: None,
        snapshot: state.clone(),
    });
    client.set_following(false).unwrap();
    assert_eq!(
        client.pending_count(),
        0,
        "authoritative state already matches"
    );
    for _ in 0..CLIENT_QUEUE_CAPACITY {
        client.send(ClientRequest::Heartbeat).unwrap();
    }
    assert_eq!(client.set_following(true), Err(SessionError::QueueFull));
    client.receive(ServerMessage::Rejected(SessionError::QueueFull));
    client.set_following(true).unwrap();
    assert_eq!(client.pending_count(), 1);
    for _ in 0..100 {
        client.set_following(true).unwrap();
    }
    assert_eq!(client.pending_count(), 1);
    client.set_following(false).unwrap();
    assert_eq!(client.pending_count(), 1, "unsent status is coalesced");
    let mut next = state;
    next.camera_epoch += 1;
    next.version += 1;
    client.receive(ServerMessage::Snapshot(next));
    client.set_following(true).unwrap();
    assert_eq!(client.pending_count(), 1);
    client.receive(ServerMessage::Left);
    assert_eq!(client.set_following(true), Err(SessionError::NotJoined));
}

#[test]
fn snapshots_reject_follow_cycles_and_missing_targets() {
    let (_, mut state, _) = follow_room();
    state.participants[0].following = Some(2);
    assert!(valid_snapshot(&state));
    state.participants[1].following = Some(1);
    assert!(!valid_snapshot(&state));
    state.participants[1].following = None;
    state.presenter = None;
    assert!(
        valid_snapshot(&state),
        "following does not require a presenter"
    );
    state.participants[0].following = Some(99);
    assert!(!valid_snapshot(&state));
}

fn target_request(state: &RoomSnapshot, target: Option<u64>) -> ClientRequest {
    ClientRequest::SetFollowing {
        room: state.room.clone(),
        presenter: state.presenter,
        epoch: state.camera_epoch,
        target,
    }
}

#[test]
fn anyone_can_follow_a_peer_but_cannot_create_a_cycle_or_force_others() {
    let (mut a, state, _) = follow_room();
    let followed = snapshot(&a.process(1, target_request(&state, Some(2)), 0).unwrap());
    assert_eq!(followed.participants[0].following, Some(2));
    assert_eq!(followed.presenter, Some(1));
    assert_eq!(followed.view_source(1), Some(2));
    assert_eq!(
        a.process(2, target_request(&state, Some(1)), 0),
        Err(SessionError::Unauthorized)
    );
    assert_eq!(
        a.process(2, target_request(&state, Some(2)), 0),
        Err(SessionError::Unauthorized)
    );
    assert_eq!(
        a.process(2, target_request(&state, Some(99)), 0),
        Err(SessionError::Unauthorized)
    );
    assert_eq!(
        a.process(2, ClientRequest::SetForcedFollow(true), 0),
        Err(SessionError::Unauthorized)
    );
    let forced = snapshot(
        &a.process(1, ClientRequest::SetForcedFollow(true), 0)
            .unwrap(),
    );
    assert!(forced.participants.iter().all(|p| p.following.is_none()));
    assert_eq!(
        a.process(1, target_request(&forced, Some(2)), 0),
        Err(SessionError::Unauthorized)
    );
    assert!(a.process(2, target_request(&forced, Some(1)), 0).is_ok());
}

#[test]
fn losing_an_independent_follow_target_clears_presence_for_everyone() {
    let (mut a, state, _) = follow_room();
    a.process(1, target_request(&state, Some(2)), 0).unwrap();
    let state = snapshot(&a.disconnect(2, 0));
    assert_eq!(state.participants[0].following, None);
    assert_eq!(state.view_source(1), Some(1));
    assert!(valid_snapshot(&state));
}

#[test]
fn presenter_can_spotlight_without_locking_followers() {
    let (mut a, state, _) = follow_room();
    let request = || {
        serde_json::from_value::<ClientRequest>(serde_json::json!("Spotlight"))
            .expect("Spotlight must be an explicit protocol action")
    };
    assert_eq!(a.process(2, request(), 0), Err(SessionError::Unauthorized));
    let messages = a.process(1, request(), 0).unwrap();
    let next = snapshot(&messages);
    assert!(!next.forced_follow);
    assert_eq!(next.presenter, Some(1));
    assert_eq!(next.document, state.document);
    assert!(!follows(&next, 2), "wait for client confirmation");
    assert!(messages.iter().any(|(peer, msg)| {
        *peer == 2
            && serde_json::to_value(msg)
                .unwrap()
                .get("Spotlight")
                .is_some()
    }));
    a.process(2, follow_request(&next, true), 0).unwrap();
    a.process(2, follow_request(&next, false), 0).unwrap();
}

#[test]
fn only_presenter_can_transfer_directly_to_a_current_participant() {
    let (mut a, state, _) = follow_room();
    let request = |id| {
        serde_json::from_value::<ClientRequest>(
            serde_json::json!({"MakePresenter": {"participant": id}}),
        )
        .expect("direct presenter transfer must be an explicit protocol action")
    };
    assert_eq!(a.process(2, request(2), 0), Err(SessionError::Unauthorized));
    assert_eq!(a.process(1, request(99), 0), Err(SessionError::Stale));
    a.process(1, ClientRequest::SetForcedFollow(true), 0)
        .unwrap();
    let next = snapshot(&a.process(1, request(2), 0).unwrap());
    assert_eq!(next.presenter, Some(2));
    assert_eq!(next.camera_epoch, state.camera_epoch + 1);
    assert!(!next.forced_follow);
    assert!(next.participants.iter().all(|p| p.following.is_none()));
    assert_eq!(next.document, state.document);
    assert_eq!(a.process(1, request(1), 0), Err(SessionError::Unauthorized));
}

#[cfg(feature = "client")]
#[test]
fn spotlight_republishes_confirmation_and_is_consumed_only_once() {
    let (mut a, state, _) = follow_room();
    let mut client = ClientSession::default();
    client.receive(ServerMessage::Welcome {
        you: 2,
        invitations: None,
        snapshot: state.clone(),
    });
    client.set_following(true).unwrap();
    for (_, message) in a.process(2, follow_request(&state, true), 0).unwrap() {
        client.receive(message);
    }
    for (peer, message) in a.process(1, ClientRequest::Spotlight, 0).unwrap() {
        if peer == 2 {
            client.receive(message);
        }
    }
    assert_eq!(client.take_spotlight(), Some(1));
    assert_eq!(client.take_spotlight(), None);
    client.set_following(true).unwrap();
    assert_eq!(client.pending_count(), 1);
    client.receive(ServerMessage::Spotlight {
        room: "retired-room".into(),
        presenter: 1,
        epoch: state.camera_epoch,
    });
    assert_eq!(client.take_spotlight(), None);
}
