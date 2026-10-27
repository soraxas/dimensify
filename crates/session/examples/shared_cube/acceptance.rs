//! Opt-in rendered acceptance harness: real WebSockets, two offscreen Dimensify apps.
use super::*;
use bevy::{
    app::PluginsState,
    camera::RenderTarget,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, save_to_disk},
    },
};
use dimensify::camera::main_camera::MainCamera;
use dimensify_session::{
    Role, SessionError,
    server::{CollaborationServerPlugin, listen},
};
use lightyear::prelude::{Linked, LocalAddr};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Capture(Handle<Image>);
fn target_camera(
    mut commands: Commands,
    target: Res<Capture>,
    cameras: Query<Entity, Added<MainCamera>>,
) {
    for camera in &cameras {
        commands
            .entity(camera)
            .insert(RenderTarget::Image(target.0.clone().into()));
    }
}
fn finish(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while app.plugins_state() != PluginsState::Ready {
        assert!(
            Instant::now() < deadline,
            "render plugins did not initialize"
        );
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::sleep(Duration::from_millis(10));
    }
    app.finish();
    app.cleanup();
}
fn client() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .build()
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::log::LogPlugin>(),
    )
    .insert_resource(Demo {
        following: true,
        ..default()
    })
    .add_plugins(CollaborationClientPlugin::<Cube>::default())
    .add_plugins(view::CubeViewPlugin)
    .add_systems(Last, target_camera);
    finish(&mut app);
    let mut image = Image::new_target_texture(960, 640, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let handle = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(Capture(handle));
    app
}
fn session(app: &App) -> &ClientSession<Cube> {
    app.world().resource::<ClientSession<Cube>>()
}
fn pump(server: &mut App, host: &mut App, viewer: &mut App, ready: impl Fn(&App, &App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        server.update();
        host.update();
        viewer.update();
        for (name, app) in [("host", &*host), ("viewer", &*viewer)] {
            assert!(
                session(app).error.is_none(),
                "{name}: {:?}; transport: {:?}",
                session(app).error,
                session(app).transport_failures()
            );
        }
        if ready(host, viewer) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "acceptance timed out: host {:?}, viewer {:?}",
            session(host).status,
            session(viewer).status
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn send(app: &mut App, request: ClientRequest<Cube>) {
    app.world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .send(request)
        .unwrap();
}
fn pose(app: &mut App) -> Transform {
    *app.world_mut()
        .query_filtered::<&Transform, With<MainCamera>>()
        .single(app.world())
        .unwrap()
}

#[test]
#[ignore = "requires a GPU and local sockets; renders offscreen, never creates an OS window"]
fn two_dimensify_clients_render_edit_follow_and_resume() {
    let output = std::env::var_os("DIMENSIFY_ACCEPTANCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/collaboration-acceptance"));
    std::fs::create_dir_all(&output).unwrap();
    let mut server = App::new();
    server
        .add_plugins(MinimalPlugins)
        .add_plugins(CollaborationServerPlugin::<Cube>::default());
    let listener = listen(server.world_mut(), "127.0.0.1:0".parse().unwrap());
    finish(&mut server);
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.world().get::<Linked>(listener).is_none() {
        server.update();
        assert!(Instant::now() < deadline, "listener did not become ready");
        std::thread::sleep(Duration::from_millis(5));
    }
    let endpoint = format!(
        "ws://{}",
        server.world().get::<LocalAddr>(listener).unwrap().0
    );
    let (mut host, mut viewer) = (client(), client());
    // Finish scene startup and initialize frame clocks before network deadlines start.
    pump(&mut server, &mut host, &mut viewer, |_, _| true);
    dimensify_session::connect::<Cube>(host.world_mut(), &endpoint).unwrap();
    send(
        &mut host,
        ClientRequest::Create {
            name: "Host".into(),
        },
    );
    pump(&mut server, &mut host, &mut viewer, |h, _| {
        session(h).status == ConnectionStatus::Joined
    });
    let room = session(&host).snapshot.as_ref().unwrap().room.clone();
    let invitation = session(&host).invitations.as_ref().unwrap().viewer.clone();
    dimensify_session::connect::<Cube>(viewer.world_mut(), &endpoint).unwrap();
    send(
        &mut viewer,
        ClientRequest::Join {
            room,
            invitation,
            name: "Viewer".into(),
        },
    );
    pump(&mut server, &mut host, &mut viewer, |_, v| {
        session(v).resume_credential().is_some()
            && session(v)
                .snapshot
                .as_ref()
                .is_some_and(|r| r.camera.is_some())
    });
    let viewer_id = session(&viewer).you.unwrap();
    let edited = Cube {
        center: [1., 0., 0.],
        color: [0.2, 0.8, 0.3],
    };
    assert_eq!(
        viewer
            .world_mut()
            .resource_mut::<ClientSession<Cube>>()
            .submit_projection::<CubeState>(edited.clone()),
        Err(SessionError::Unauthorized)
    );
    host.world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .submit_projection::<CubeState>(edited.clone())
        .unwrap();
    pump(&mut server, &mut host, &mut viewer, |_, v| {
        session(v).snapshot.as_ref().unwrap().document.data == edited
    });
    send(
        &mut host,
        ClientRequest::SetParticipantRole {
            participant: viewer_id,
            role: Role::Editor,
        },
    );
    pump(&mut server, &mut host, &mut viewer, |_, v| {
        session(v)
            .snapshot
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .any(|p| p.id == viewer_id && p.role == Role::Editor)
    });
    let reverse = Cube {
        center: [-1., 0., 0.],
        color: [0.8, 0.3, 0.2],
    };
    viewer
        .world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .submit_projection::<CubeState>(reverse.clone())
        .unwrap();
    pump(&mut server, &mut host, &mut viewer, |h, _| {
        session(h).snapshot.as_ref().unwrap().document.data == reverse
    });
    let new_pose = Transform::from_xyz(3., -5., 4.).looking_at(Vec3::ZERO, Vec3::Z);
    *host
        .world_mut()
        .query_filtered::<&mut Transform, With<MainCamera>>()
        .single_mut(host.world_mut())
        .unwrap() = new_pose;
    pump(&mut server, &mut host, &mut viewer, |_, v| {
        session(v)
            .snapshot
            .as_ref()
            .unwrap()
            .camera
            .as_ref()
            .is_some_and(|c| c.pose.position == new_pose.translation.to_array())
    });
    assert!(pose(&mut viewer).translation.distance(new_pose.translation) < 0.001);
    viewer.world_mut().resource_mut::<Demo>().following = false;
    let local = Transform::from_xyz(-4., 2., 3.).looking_at(Vec3::ZERO, Vec3::Z);
    *viewer
        .world_mut()
        .query_filtered::<&mut Transform, With<MainCamera>>()
        .single_mut(viewer.world_mut())
        .unwrap() = local;
    pump(&mut server, &mut host, &mut viewer, |_, _| true);
    assert!(pose(&mut viewer).translation.distance(local.translation) < 0.001);
    assert!(pose(&mut host).translation.distance(new_pose.translation) < 0.001);
    viewer.world_mut().resource_mut::<Demo>().following = true;
    let credential = session(&viewer).resume_credential().unwrap().clone();
    dimensify_session::disconnect::<Cube>(viewer.world_mut());
    viewer
        .world_mut()
        .resource_mut::<ClientSession<Cube>>()
        .restore_resume_credential(credential);
    dimensify_session::connect::<Cube>(viewer.world_mut(), &endpoint).unwrap();
    pump(&mut server, &mut host, &mut viewer, |_, v| {
        session(v).status == ConnectionStatus::Joined
    });
    assert_eq!(session(&viewer).you, Some(viewer_id));
    assert_eq!(
        session(&viewer)
            .snapshot
            .as_ref()
            .unwrap()
            .participants
            .iter()
            .find(|p| p.id == viewer_id)
            .unwrap()
            .role,
        Role::Editor
    );
    for _ in 0..60 {
        pump(&mut server, &mut host, &mut viewer, |_, _| true);
    }
    for (app, name) in [(&mut host, "host.png"), (&mut viewer, "viewer.png")] {
        let _ = std::fs::remove_file(output.join(name));
        let target = app.world().resource::<Capture>().0.clone();
        app.world_mut()
            .spawn(Screenshot::image(target))
            .observe(save_to_disk(output.join(name)));
    }
    pump(&mut server, &mut host, &mut viewer, |_, _| {
        output.join("host.png").exists() && output.join("viewer.png").exists()
    });
    println!(
        "Offscreen two-client acceptance passed; images: {}",
        output.display()
    );
}
