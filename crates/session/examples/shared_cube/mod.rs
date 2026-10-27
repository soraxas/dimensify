//! Two independent viewers sharing a room. See docs/examples.md in the workspace.
#[cfg(all(test, feature = "server", feature = "example-dimensify"))]
mod acceptance;
#[path = "../common/mod.rs"]
mod common;
mod view;
use bevy::prelude::*;
use common::{Cube, CubeState};
use dimensify_session::{
    ClientRequest, ClientSession, CollaborationClientPlugin, CollaborationSystems,
    ConnectionStatus, Invitation, InviteCode,
};

#[derive(Resource)]
struct Options {
    endpoint: String,
    request: ClientRequest<Cube>,
    invite_codes: bool,
}
#[derive(Resource, Default)]
struct Demo {
    announced: bool,
    following: bool,
    last_error: Option<dimensify_session::SessionError>,
}
/// Run the shared scene with either short codes or explicit room/token invitations.
pub(super) fn run(invite_codes: bool) {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let usage = if invite_codes {
        "shared_cube_invite [ENDPOINT] [CODE [NAME]]\nOmit code to create. Copy a printed join command into another terminal."
    } else {
        "shared_cube [ENDPOINT] [ROOM INVITATION [NAME]]\nOmit room/invitation to create. Copy a printed join command into another terminal."
    };
    if args.first().is_some_and(|s| s == "--help") {
        println!("{usage}\nDefault endpoint: ws://127.0.0.1:5002");
        return;
    }
    let valid = if invite_codes {
        matches!(args.len(), 0..=3)
    } else {
        matches!(args.len(), 0 | 1 | 3 | 4)
    };
    if !valid {
        eprintln!("{usage}");
        std::process::exit(2);
    }
    let endpoint = args
        .first()
        .cloned()
        .unwrap_or_else(|| "ws://127.0.0.1:5002".into());
    let request = if invite_codes && args.len() >= 2 {
        let code = InviteCode::parse(&args[1]).unwrap_or_else(|_| {
            eprintln!("Invalid invite code: expected 12 characters, such as ABCD-EFGH-JKMN.");
            std::process::exit(2);
        });
        ClientRequest::JoinWithCode {
            code,
            name: args.get(2).cloned().unwrap_or_else(|| "guest".into()),
        }
    } else if let Some([room, invitation]) = args.get(1..3) {
        ClientRequest::Join {
            room: room.clone(),
            invitation: Invitation(invitation.clone()),
            name: args.get(3).cloned().unwrap_or_else(|| "guest".into()),
        }
    } else {
        ClientRequest::Create {
            name: "host".into(),
        }
    };
    App::new()
        .insert_resource(Options {
            endpoint,
            request,
            invite_codes,
        })
        .insert_resource(Demo {
            following: true,
            ..default()
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Shared cube - independent collaboration example".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CollaborationClientPlugin::<Cube>::default())
        .add_plugins(view::CubeViewPlugin)
        .add_systems(Startup, join)
        .add_systems(
            Update,
            (announce, controls)
                .chain()
                .after(CollaborationSystems::Transport),
        )
        .run();
}
fn join(world: &mut World) {
    let options = world.resource::<Options>();
    let endpoint = options.endpoint.clone();
    let request = options.request.clone();
    let result = dimensify_session::connect::<Cube>(world, &endpoint)
        .and_then(|()| world.resource_mut::<ClientSession<Cube>>().send(request));
    if let Err(error) = result {
        eprintln!("Could not connect: {error}");
    }
}
fn announce(options: Res<Options>, session: Res<ClientSession<Cube>>, mut demo: ResMut<Demo>) {
    if !demo.announced && session.status == ConnectionStatus::Joined {
        demo.announced = true;
        if let (Some(room), Some(invites)) = (&session.snapshot, &session.invitations) {
            // Keep the second peer on the same viewer implementation as the host.
            let feature = if cfg!(feature = "example-dimensify") {
                "example-dimensify"
            } else {
                "example-viewer"
            };
            let endpoint = options.endpoint.replace('\'', "'\\''");
            if options.invite_codes {
                if let Some(codes) = &invites.codes {
                    for (role, code) in [("viewer", &codes.viewer), ("editor", &codes.editor)] {
                        println!(
                            "Join as {role}:\ncargo run --locked -p dimensify_session --features {feature} --example shared_cube_invite -- '{}' {} {role}",
                            endpoint,
                            code.display_code()
                        );
                    }
                } else {
                    eprintln!("This server did not issue short invite codes.");
                }
            } else {
                for (role, invitation) in [("viewer", &invites.viewer), ("editor", &invites.editor)]
                {
                    println!(
                        "Join as {role}:\ncargo run --locked -p dimensify_session --features {feature} --example shared_cube -- '{}' {} {} {role}",
                        endpoint, room.room, invitation.0
                    );
                }
            }
        }
    }
    if session.error != demo.last_error {
        demo.last_error = session.error;
        if let Some(error) = session.error {
            eprintln!("Request rejected: {error}");
        }
    }
}
fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<ClientSession<Cube>>,
    mut demo: ResMut<Demo>,
) {
    let Some(room) = session.snapshot.as_ref() else {
        return;
    };
    let you = session.you;
    if keys.just_pressed(KeyCode::Space) || keys.just_pressed(KeyCode::KeyX) {
        let mut data = room.document.data.clone();
        if keys.just_pressed(KeyCode::Space) {
            data.color.rotate_left(1);
        } else {
            data.center[0] = if data.center[0] > 0. { -1. } else { 1. };
        }
        if let Err(error) = session.submit_projection::<CubeState>(data) {
            eprintln!("Cube edit: {error}");
        }
        return;
    }
    let request = if keys.just_pressed(KeyCode::KeyP) {
        Some(ClientRequest::RequestPresenter)
    } else if keys.just_pressed(KeyCode::KeyG) && room.presenter == you {
        room.presenter_requests
            .first()
            .map(|id| ClientRequest::GrantPresenter { request: *id })
    } else if keys.just_pressed(KeyCode::KeyR) {
        Some(ClientRequest::ClaimPresenter)
    } else {
        None
    };
    if keys.just_pressed(KeyCode::KeyF) {
        demo.following = true;
    }
    if let Some(request) = request {
        if let Err(error) = session.send(request) {
            eprintln!("Could not send: {error}");
        }
    }
}
