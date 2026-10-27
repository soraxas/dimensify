#![cfg(all(feature = "client", feature = "server", not(target_family = "wasm")))]
use bevy::prelude::*;
use dimensify_session::{server::*, *};
use lightyear::prelude::{Linked, LocalAddr};
use std::time::{Duration, Instant};
#[path = "../examples/common/mod.rs"]
mod common;
use common::Cube;

fn until(apps: &mut [&mut App], done: impl Fn(&[&mut App]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(apps) {
        for app in apps.iter_mut() {
            app.update();
        }
        assert!(Instant::now() < deadline, "session operation timed out");
        std::thread::sleep(Duration::from_millis(3));
    }
}
fn client(endpoint: &str, request: ClientRequest<Cube>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(CollaborationClientPlugin::<Cube>::default());
    app.finish();
    app.cleanup();
    connect::<Cube>(app.world_mut(), endpoint).unwrap();
    app.world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .send(request)
        .unwrap();
    app
}
fn joined(app: &App) -> bool {
    app.world().resource::<ClientSession<Cube>>().status == ConnectionStatus::Joined
}

#[test]
fn websocket_create_code_join_and_owner_renewal_reach_clients() {
    let mut server = App::new();
    server
        .add_plugins(MinimalPlugins)
        .add_plugins(CollaborationServerPlugin::<Cube>::default());
    let listener = listen(server.world_mut(), "127.0.0.1:0".parse().unwrap());
    server.finish();
    server.cleanup();
    until(&mut [&mut server], |apps| {
        apps[0].world().get::<Linked>(listener).is_some()
    });
    let endpoint = format!(
        "ws://{}",
        server.world().get::<LocalAddr>(listener).unwrap().0
    );
    let mut host = client(
        &endpoint,
        ClientRequest::Create {
            name: "Host".into(),
        },
    );
    until(&mut [&mut server, &mut host], |apps| joined(apps[1]));
    let old = host
        .world()
        .resource::<ClientSession<Cube>>()
        .invitations
        .as_ref()
        .unwrap()
        .codes
        .clone()
        .unwrap();
    let mut viewer = client(
        &endpoint,
        ClientRequest::JoinWithCode {
            code: old.viewer.clone(),
            name: "Viewer".into(),
        },
    );
    until(&mut [&mut server, &mut host, &mut viewer], |apps| {
        joined(apps[2])
    });
    let session = viewer.world().resource::<ClientSession<Cube>>();
    assert!(session.invitations.is_none());
    assert_eq!(
        session
            .snapshot
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .find(|p| Some(p.id) == session.you)
            .unwrap()
            .role,
        Role::Viewer
    );
    host.world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .send(ClientRequest::RenewInviteCodes)
        .unwrap();
    until(&mut [&mut server, &mut host, &mut viewer], |apps| {
        apps[1]
            .world()
            .resource::<ClientSession<Cube>>()
            .invitations
            .as_ref()
            .unwrap()
            .codes
            .as_ref()
            != Some(&old)
    });
    assert!(joined(&viewer));
    let mut invalid = client(
        &endpoint,
        ClientRequest::JoinWithCode {
            code: old.editor,
            name: "Expired".into(),
        },
    );
    until(
        &mut [&mut server, &mut host, &mut viewer, &mut invalid],
        |apps| {
            apps[3]
                .world()
                .resource::<ClientSession<Cube>>()
                .error
                .is_some()
        },
    );
    assert_eq!(
        invalid.world().resource::<ClientSession<Cube>>().error,
        Some(SessionError::Unauthorized)
    );
}
