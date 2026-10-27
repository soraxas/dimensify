#![cfg(all(feature = "server", feature = "client"))]
use dimensify_session::*;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct Scene;
impl SharedDocument for Scene {
    const SCHEMA: &'static str = "participant-views-test.v1";
    fn validate(&self) -> bool {
        true
    }
    fn registrations() -> RegistrationSet<Self> {
        RegistrationSet::default().presentation_camera()
    }
}

fn room() -> (
    authority::Authority<Scene>,
    RoomSnapshot<Scene>,
    ResumeCredential,
) {
    let mut authority = authority::Authority::default();
    let created = authority
        .process(
            1,
            ClientRequest::Create {
                name: "Presenter".into(),
            },
            0,
        )
        .unwrap();
    let (state, invitations) = created
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome {
                snapshot,
                invitations: Some(invites),
                ..
            } => Some((snapshot, invites)),
            _ => None,
        })
        .unwrap();
    let joined = authority
        .process(
            2,
            ClientRequest::Join {
                room: state.room.clone(),
                invitation: invitations.viewer.clone(),
                name: "Explorer".into(),
            },
            0,
        )
        .unwrap();
    let state = joined
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome { snapshot, .. } => Some(snapshot.clone()),
            _ => None,
        })
        .unwrap();
    let credential = joined
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::ResumeCredential(c) => Some(c.clone()),
            _ => None,
        })
        .unwrap();
    (authority, state, credential)
}

fn pose() -> CameraPose {
    CameraPose {
        position: [1., 2., 3.],
        rotation: [0., 0., 0., 1.],
        projection: Projection::Perspective { vertical_fov: 0.8 },
        focus_distance: 5.,
    }
}

#[test]
fn views_are_authenticated_validated_and_sequenced_across_reconnects() {
    let (mut authority, state, credential) = room();
    let sample = CameraSnapshot {
        room: state.room.clone(),
        epoch: state.camera_epoch,
        scene_revision: state.document.revision,
        sequence: 10,
        pose: pose(),
    };
    assert_eq!(
        authority.process(99, ClientRequest::PublishView(sample.clone()), 0),
        Err(SessionError::NotJoined)
    );
    let replies = authority
        .process(2, ClientRequest::PublishView(sample.clone()), 0)
        .unwrap();
    let [
        (
            recipient,
            ServerMessage::View {
                participant,
                camera,
            },
        ),
    ] = replies.as_slice()
    else {
        panic!()
    };
    assert_eq!((*recipient, *participant), (1, 2));
    let sequence = camera.sequence;
    assert_eq!(
        authority.process(2, ClientRequest::PublishView(sample.clone()), 0),
        Err(SessionError::Stale)
    );
    let mut invalid = sample.clone();
    invalid.pose.rotation = [0.; 4];
    assert_eq!(
        authority.process(2, ClientRequest::PublishView(invalid), 0),
        Err(SessionError::Invalid)
    );
    let mut stale = sample.clone();
    stale.scene_revision += 1;
    assert_eq!(
        authority.process(2, ClientRequest::PublishView(stale), 0),
        Err(SessionError::Stale)
    );
    authority
        .process(3, ClientRequest::Resume { credential }, 0)
        .unwrap();
    let mut restarted = sample;
    restarted.sequence = 1;
    let replies = authority
        .process(3, ClientRequest::PublishView(restarted), 0)
        .unwrap();
    let [
        (
            _,
            ServerMessage::View {
                participant,
                camera,
            },
        ),
    ] = replies.as_slice()
    else {
        panic!()
    };
    assert_eq!(*participant, 2);
    assert!(
        camera.sequence > sequence,
        "a reconnected peer must not get stuck behind its old sequence"
    );
}

#[test]
fn client_camera_updates_work_without_cursor_subscription_and_recover_settled_views() {
    let (_, state, _) = room();
    let mut client = ClientSession::default();
    client.receive(ServerMessage::Welcome {
        you: 2,
        invitations: None,
        snapshot: state,
    });
    assert!(client.cursor_subscription().is_none());
    assert!(client.publish_view(pose(), Duration::ZERO).unwrap());
    assert!(
        !client
            .publish_view(pose(), Duration::from_millis(50))
            .unwrap()
    );
    assert!(client.publish_view(pose(), Duration::from_secs(1)).unwrap());
    assert_eq!(client.pending_count(), 1, "latest values coalesce");
    let mut moved = pose();
    moved.position[0] += 1.;
    assert!(
        !client
            .publish_view(moved.clone(), Duration::from_millis(1020))
            .unwrap()
    );
    assert!(
        client
            .publish_view(moved, Duration::from_millis(1050))
            .unwrap()
    );
}

#[test]
fn peer_views_cannot_cross_rooms_revisions_or_departed_identities() {
    let (_, state, _) = room();
    let mut client = ClientSession::default();
    client.receive(ServerMessage::Welcome {
        you: 1,
        invitations: None,
        snapshot: state.clone(),
    });
    let sample = CameraSnapshot {
        room: state.room.clone(),
        epoch: state.camera_epoch,
        scene_revision: state.document.revision,
        sequence: 2,
        pose: pose(),
    };
    client.receive(ServerMessage::View {
        participant: 2,
        camera: sample.clone(),
    });
    assert_eq!(client.participant_view(2), Some(&sample));
    let mut older = sample.clone();
    older.sequence = 1;
    older.pose.position[0] = 99.;
    client.receive(ServerMessage::View {
        participant: 2,
        camera: older,
    });
    assert_eq!(client.participant_view(2), Some(&sample));
    let mut next = state;
    next.version += 1;
    next.participants.retain(|p| p.id != 2);
    client.receive(ServerMessage::Snapshot(next));
    client.receive(ServerMessage::View {
        participant: 2,
        camera: sample,
    });
    assert!(client.participant_view(2).is_none());
}
