use dimensify_session::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct State {
    shared: u32,
    local: u32,
    presenter: u32,
}
struct Shared;
impl DocumentProjection<State> for Shared {
    type Payload = u32;
    const ID: &'static str = "shared";
    const VERSION: u32 = 1;
    fn read(d: &State) -> u32 {
        d.shared
    }
    fn replace(d: &mut State, p: u32) {
        d.shared = p;
    }
    fn validate(p: &u32) -> bool {
        *p < 100
    }
}
struct Presenter;
impl DocumentProjection<State> for Presenter {
    type Payload = u32;
    const ID: &'static str = "presenter";
    const VERSION: u32 = 1;
    fn read(d: &State) -> u32 {
        d.presenter
    }
    fn replace(d: &mut State, p: u32) {
        d.presenter = p;
    }
    fn validate(_: &u32) -> bool {
        true
    }
}
impl SharedDocument for State {
    const SCHEMA: &'static str = "registration.test";
    fn validate(&self) -> bool {
        true
    }
    fn registrations() -> RegistrationSet<Self> {
        RegistrationSet::default()
            .document::<Shared>(WritePolicy::Editors)
            .document::<Presenter>(WritePolicy::Presenter)
            .presentation_camera()
    }
}
#[test]
fn projections_enforce_policy_validation_and_excluded_fields() {
    let r = State::registrations();
    let old = State::default();
    let mut next = old.clone();
    next.shared = 1;
    assert_eq!(r.apply(&old, &next, true, false), Ok(next.clone()));
    assert_eq!(
        r.apply(&old, &next, false, true),
        Err(SessionError::Unauthorized)
    );
    next.shared = 100;
    assert_eq!(
        r.apply(&old, &next, true, false),
        Err(SessionError::Invalid)
    );
    next.shared = 0;
    next.local = 1;
    assert_eq!(
        r.apply(&old, &next, true, true),
        Err(SessionError::Unauthorized)
    );
    next.local = 0;
    next.presenter = 1;
    assert_eq!(r.apply(&old, &next, false, true), Ok(next));
    assert_eq!(
        r.apply(&old, &old, false, false),
        Err(SessionError::Unauthorized)
    );
}
#[test]
fn manifest_is_canonical_and_duplicates_fail_closed() {
    let a = State::registrations();
    let b = RegistrationSet::<State>::default()
        .presentation_camera()
        .document::<Presenter>(WritePolicy::Presenter)
        .document::<Shared>(WritePolicy::Editors);
    assert_eq!(a.manifest(), b.manifest());
    let duplicate = a.document::<Shared>(WritePolicy::Editors);
    assert_eq!(duplicate.manifest(), Err(SessionError::Invalid));
    assert_eq!(
        duplicate.apply(&State::default(), &State::default(), true, true),
        Err(SessionError::Invalid)
    );
}

#[cfg(all(feature = "client", feature = "server"))]
#[test]
fn unrelated_control_rejection_keeps_pending_projection_until_correlated_outcome() {
    let mut authority = authority::Authority::<State>::default();
    let mut client = ClientSession::<State>::default();
    for (_, m) in authority
        .process(
            1,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap()
    {
        client.receive(m);
    }
    let id = client.submit_projection::<Shared>(1).unwrap().unwrap();
    client.receive(ServerMessage::Rejected(SessionError::Unauthorized));
    assert_eq!(client.pending_document_edit(), Some(id));
    assert_eq!(client.pending_count(), 1);
    client.set_cursor_sharing(true).unwrap();
    client.set_cursor_sharing(false).unwrap();
    assert_eq!(
        client.pending_count(),
        2,
        "cursor toggles retain the queued document edit"
    );
    client.receive(ServerMessage::DocumentEditOutcome(
        DocumentEditOutcome::Rejected {
            id: EditId(id.0 + 1),
            error: SessionError::Conflict,
        },
    ));
    assert_eq!(client.pending_document_edit(), Some(id));
    client.receive(ServerMessage::DocumentEditOutcome(
        DocumentEditOutcome::Rejected {
            id,
            error: SessionError::Conflict,
        },
    ));
    assert_eq!(client.pending_document_edit(), None);
    assert_eq!(client.submit_projection::<Shared>(0), Ok(None));
}

#[derive(bevy::prelude::Resource, Default)]
struct Installed(u32);
struct Adapter;
impl SharedAdapter for Adapter {
    type Document = State;
    const ID: &'static str = "shared";
    const VERSION: u32 = 1;
    const POLICY: WritePolicy = WritePolicy::Editors;
    const CHANNEL: SharedChannel = SharedChannel::Document;
    fn install(app: &mut bevy::prelude::App) {
        app.init_resource::<Installed>();
        app.world_mut().resource_mut::<Installed>().0 += 1;
    }
}
struct DuplicateAdapter;
impl SharedAdapter for DuplicateAdapter {
    type Document = State;
    const ID: &'static str = "shared";
    const VERSION: u32 = 1;
    const POLICY: WritePolicy = WritePolicy::Editors;
    const CHANNEL: SharedChannel = SharedChannel::Document;
    fn install(_: &mut bevy::prelude::App) {
        panic!("conflicting adapter must not install");
    }
}
#[test]
fn adapter_installation_is_idempotent_and_rejects_descriptor_conflicts() {
    let mut app = bevy::prelude::App::new();
    app.register_shared::<Adapter>().unwrap();
    app.register_shared::<Adapter>().unwrap();
    assert_eq!(app.world().resource::<Installed>().0, 1);
    assert!(app.register_shared::<DuplicateAdapter>().is_err());
}

#[cfg(all(feature = "client", feature = "server"))]
#[test]
fn reconnect_credentials_are_bound_to_normalized_endpoint() {
    use bevy::prelude::{App, MinimalPlugins};
    let mut a = authority::Authority::<State>::default();
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
        .into_iter()
        .find_map(|(_, m)| match m {
            ServerMessage::ResumeCredential(c) => Some(c),
            _ => None,
        })
        .unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(CollaborationClientPlugin::<State>::default());
    connect::<State>(app.world_mut(), "ws://EXAMPLE.test:80/socket").unwrap();
    app.world_mut()
        .resource_mut::<ClientSession<State>>()
        .restore_resume_credential(credential.clone());
    connect::<State>(app.world_mut(), "ws://example.test/socket").unwrap();
    assert_eq!(
        app.world()
            .resource::<ClientSession<State>>()
            .resume_credential(),
        Some(&credential)
    );
    connect::<State>(app.world_mut(), "ws://different.test/socket").unwrap();
    assert!(
        app.world()
            .resource::<ClientSession<State>>()
            .resume_credential()
            .is_none()
    );
}

#[cfg(all(feature = "client", feature = "server"))]
#[test]
fn server_correlates_conflicts_and_acceptance_and_client_rebases_camera_samples() {
    use std::time::Duration;
    let mut a = authority::Authority::<State>::default();
    let mut client = ClientSession::<State>::default();
    for (_, m) in a
        .process(
            100,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap()
    {
        client.receive(m);
    }
    let id = client.submit_projection::<Shared>(1).unwrap().unwrap();
    let accepted = a
        .process(
            100,
            ClientRequest::SubmitDocument {
                id,
                expected_revision: 0,
                data: State {
                    shared: 1,
                    ..Default::default()
                },
            },
            0,
        )
        .unwrap();
    for (_, m) in accepted {
        client.receive(m);
    }
    assert_eq!(
        client.document_edit_outcome,
        Some(DocumentEditOutcome::Accepted { id, revision: 1 })
    );
    assert_eq!(client.snapshot.as_ref().unwrap().document.data.shared, 1);
    let rejected = a
        .process(
            100,
            ClientRequest::SubmitDocument {
                id: EditId(99),
                expected_revision: 0,
                data: State {
                    shared: 2,
                    ..Default::default()
                },
            },
            0,
        )
        .unwrap();
    assert!(rejected.iter().any(|(_, m)| *m
        == ServerMessage::DocumentEditOutcome(DocumentEditOutcome::Rejected {
            id: EditId(99),
            error: SessionError::Conflict
        })));
    let pose = CameraPose {
        position: [0.; 3],
        rotation: [0., 0., 0., 1.],
        projection: Projection::Perspective { vertical_fov: 1. },
        focus_distance: 1.,
    };
    assert_eq!(
        client.publish_camera_sample(pose.clone(), Duration::ZERO),
        Ok(true)
    );
    assert_eq!(
        client.publish_camera_sample(pose.clone(), Duration::from_secs(1)),
        Ok(false)
    );
    let mut moved = pose;
    moved.position[0] = 1.;
    assert_eq!(
        client.publish_camera_sample(moved.clone(), Duration::from_millis(10)),
        Ok(false)
    );
    assert_eq!(
        client.publish_camera_sample(moved, Duration::from_millis(60)),
        Ok(true)
    );
}

#[cfg(feature = "server")]
#[test]
fn viewer_granted_presentation_can_change_presenter_projection_but_not_editor_projection() {
    let mut a = authority::Authority::<State>::default();
    let host = a
        .process(
            100,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap();
    let (room, invitation) = host
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome {
                snapshot,
                invitations: Some(i),
                ..
            } => Some((snapshot.room.clone(), i.viewer.clone())),
            _ => None,
        })
        .unwrap();
    let joined = a
        .process(
            200,
            ClientRequest::Join {
                room,
                invitation,
                name: "Viewer".into(),
            },
            0,
        )
        .unwrap();
    let _viewer = joined
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome { you, .. } => Some(*you),
            _ => None,
        })
        .unwrap();
    let request = a
        .process(200, ClientRequest::RequestPresenter, 0)
        .unwrap()
        .into_iter()
        .find_map(|(_, m)| match m {
            ServerMessage::PresenterRequested(request) => Some(request),
            _ => None,
        })
        .unwrap();
    a.process(100, ClientRequest::GrantPresenter { request }, 0)
        .unwrap();
    assert!(
        a.process(
            200,
            ClientRequest::SetDocument {
                expected_revision: 0,
                data: State {
                    presenter: 1,
                    ..Default::default()
                }
            },
            0
        )
        .is_ok()
    );
    assert_eq!(
        a.process(
            200,
            ClientRequest::SetDocument {
                expected_revision: 1,
                data: State {
                    presenter: 1,
                    shared: 1,
                    local: 0
                }
            },
            0
        ),
        Err(SessionError::Unauthorized)
    );
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Local(u32);
impl SharedDocument for Local {
    const SCHEMA: &'static str = "local";
    fn validate(&self) -> bool {
        true
    }
}
#[cfg(feature = "server")]
#[test]
fn default_registration_shares_no_document_or_camera_lane() {
    let mut a = authority::Authority::<Local>::default();
    let messages = a
        .process(
            1,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap();
    let room = messages
        .iter()
        .find_map(|(_, m)| match m {
            ServerMessage::Welcome { snapshot, .. } => Some(snapshot.room.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        a.process(
            1,
            ClientRequest::SetDocument {
                expected_revision: 0,
                data: Local(1)
            },
            0
        ),
        Err(SessionError::Unauthorized)
    );
    let pose = CameraPose {
        position: [0.; 3],
        rotation: [0., 0., 0., 1.],
        projection: Projection::Perspective { vertical_fov: 1. },
        focus_distance: 1.,
    };
    assert_eq!(
        a.process(
            1,
            ClientRequest::PublishCamera(CameraSnapshot {
                room,
                epoch: 1,
                scene_revision: 0,
                sequence: 1,
                pose
            }),
            0
        ),
        Err(SessionError::Unauthorized)
    );
}

#[cfg(all(feature = "client", feature = "server"))]
#[test]
fn leave_discards_pending_edit_and_new_room_ignores_old_outcome() {
    let mut authority = authority::Authority::<State>::default();
    let messages = authority
        .process(
            100,
            ClientRequest::Create {
                name: "Host".into(),
            },
            0,
        )
        .unwrap();
    let welcome = messages
        .iter()
        .find_map(|(_, m)| matches!(m, ServerMessage::Welcome { .. }).then_some(m.clone()))
        .unwrap();
    let mut client = ClientSession::<State>::default();
    client.receive(welcome.clone());
    client.leave().unwrap();
    let old = client.submit_projection::<Shared>(1).unwrap().unwrap();
    client.receive(ServerMessage::Left);
    assert_eq!(client.pending_document_edit(), None);
    client.receive(welcome);
    let new = client.submit_projection::<Shared>(2).unwrap().unwrap();
    assert!(new.0 > old.0);
    client.receive(ServerMessage::DocumentEditOutcome(
        DocumentEditOutcome::Rejected {
            id: old,
            error: SessionError::NotJoined,
        },
    ));
    assert_eq!(client.pending_document_edit(), Some(new));
}
