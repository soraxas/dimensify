#![cfg(all(feature = "client", feature = "server"))]
use dimensify_session::{authority::Authority, *};
#[path = "../examples/common/mod.rs"]
mod common;
use common::Cube;

#[test]
fn application_document_validates_edits_and_reaches_newcomers() {
    let mut authority = Authority::<Cube>::default();
    let mut host = ClientSession::<Cube>::default();
    for (_, msg) in authority
        .process(
            1,
            ClientRequest::Create {
                name: "host".into(),
            },
            0,
        )
        .unwrap()
    {
        host.receive(msg);
    }
    let room = host.snapshot.as_ref().unwrap().room.clone();
    let invites = host.invitations.clone().unwrap();
    authority
        .process(
            2,
            ClientRequest::Join {
                room: room.clone(),
                invitation: invites.viewer,
                name: "viewer".into(),
            },
            0,
        )
        .unwrap();
    let edit = |x, expected_revision| ClientRequest::SetDocument {
        expected_revision,
        data: Cube {
            center: [x, 0., 0.],
            ..Default::default()
        },
    };
    assert_eq!(
        authority.process(2, edit(1., 0), 0),
        Err(SessionError::Unauthorized)
    );
    assert_eq!(
        authority.process(1, edit(f32::NAN, 0), 0),
        Err(SessionError::Invalid)
    );
    for (_, msg) in authority
        .process(1, edit(3., 0), 0)
        .unwrap()
        .into_iter()
        .filter(|(p, _)| *p == 1)
    {
        host.receive(msg);
    }
    assert_eq!(host.snapshot.as_ref().unwrap().document.data.center[0], 3.);
    assert_eq!(host.snapshot.as_ref().unwrap().document.revision, 1);
    assert_eq!(
        authority.process(1, edit(4., 0), 0),
        Err(SessionError::Conflict)
    );
    let mut newcomer = ClientSession::<Cube>::default();
    for (_, msg) in authority
        .process(
            3,
            ClientRequest::Join {
                room,
                invitation: invites.editor,
                name: "editor".into(),
            },
            0,
        )
        .unwrap()
        .into_iter()
        .filter(|(p, _)| *p == 3)
    {
        newcomer.receive(msg);
    }
    assert_eq!(
        newcomer.snapshot.as_ref().unwrap().document.data.center[0],
        3.
    );
    assert!(authority.process(3, edit(-2., 1), 0).is_ok());
}

#[test]
fn invalid_application_snapshots_never_replace_accepted_state() {
    let mut authority = Authority::<Cube>::default();
    let (_, welcome) = authority
        .process(
            1,
            ClientRequest::Create {
                name: "host".into(),
            },
            0,
        )
        .unwrap()
        .into_iter()
        .find(|(_, m)| matches!(m, ServerMessage::Welcome { .. }))
        .unwrap();
    let ServerMessage::Welcome { snapshot, .. } = &welcome else {
        panic!()
    };
    let mut invalid = snapshot.clone();
    invalid.document.data.color[0] = 2.;
    let mut session = ClientSession::<Cube>::default();
    session.receive(ServerMessage::Welcome {
        you: 1,
        invitations: None,
        snapshot: invalid.clone(),
    });
    assert!(session.snapshot.is_none());
    assert_eq!(session.error, Some(SessionError::Invalid));
    session.receive(welcome);
    let accepted = session.snapshot.clone();
    invalid.version += 1;
    invalid.document.revision += 1;
    session.receive(ServerMessage::Snapshot(invalid));
    assert_eq!(session.error, Some(SessionError::Invalid));
    assert_eq!(session.snapshot, accepted);
}

#[test]
fn leave_then_viewer_join_never_recovers_owner_identity() {
    let mut a = Authority::<common::Cube>::default();
    let host = a
        .process(
            1,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap();
    let (owner, room, invite) = host
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome {
                you,
                snapshot,
                invitations: Some(i),
            } => Some((*you, snapshot.room.clone(), i.viewer.clone())),
            _ => None,
        })
        .unwrap();
    a.process(1, ClientRequest::Leave, 1).unwrap();
    let joined = a
        .process(
            1,
            ClientRequest::Join {
                room,
                invitation: invite,
                name: "Viewer".into(),
            },
            2,
        )
        .unwrap();
    let you = joined
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome { you, .. } => Some(*you),
            _ => None,
        })
        .unwrap();
    assert_ne!(you, owner);
    assert_eq!(
        a.process(
            1,
            ClientRequest::SetParticipantRole {
                participant: you,
                role: Role::Editor
            },
            3
        ),
        Err(SessionError::Unauthorized)
    );
}

#[test]
fn presenter_decisions_are_generation_safe_and_dismiss_allows_request_again() {
    let mut a = Authority::<Cube>::default();
    let mut host = ClientSession::<Cube>::default();
    for (_, m) in a
        .process(
            1,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap()
    {
        host.receive(m);
    }
    let room = host.snapshot.as_ref().unwrap().room.clone();
    a.process(
        2,
        ClientRequest::Join {
            room,
            invitation: host.invitations.as_ref().unwrap().viewer.clone(),
            name: "Viewer".into(),
        },
        0,
    )
    .unwrap();
    let request = |a: &mut Authority<Cube>| {
        a.process(2, ClientRequest::RequestPresenter, 0)
            .unwrap()
            .into_iter()
            .find_map(|(_, m)| match m {
                ServerMessage::Snapshot(s) => s.presenter_requests.first().copied(),
                _ => None,
            })
            .unwrap()
    };
    let old = request(&mut a);
    assert_eq!(
        a.process(2, ClientRequest::DismissPresenter { request: old }, 0),
        Err(SessionError::Unauthorized)
    );
    let messages = a
        .process(1, ClientRequest::DismissPresenter { request: old }, 0)
        .unwrap();
    assert!(messages.iter().any(|(peer,m)| *peer == 2 && matches!(m, ServerMessage::PresenterDecision { request, granted: false } if *request == old)));
    let new = request(&mut a);
    assert_ne!(new.id, old.id);
    assert_eq!(
        a.process(1, ClientRequest::GrantPresenter { request: old }, 0),
        Err(SessionError::Stale)
    );
    let messages = a
        .process(1, ClientRequest::GrantPresenter { request: new }, 0)
        .unwrap();
    let state = messages
        .into_iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Snapshot(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(state.presenter, Some(new.participant));
    assert_eq!(
        state
            .participants
            .iter()
            .find(|p| p.id == new.participant)
            .unwrap()
            .role,
        Role::Viewer
    );
}
