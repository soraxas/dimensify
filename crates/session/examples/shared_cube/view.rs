//! Local rendering/input adapter. Only Cube and camera/cursor samples cross the wire.
use super::{Cube, Demo};
use bevy::{prelude::*, window::PrimaryWindow};
#[cfg(test)]
use dimensify_session::CameraSnapshot;
use dimensify_session::{
    CAMERA_ID, CAMERA_VERSION, CameraPose, ClientSession, CollaborationSystems, DocumentProjection,
    SharedAdapter, SharedAppExt, SharedChannel, WritePolicy,
};

#[derive(Component)]
struct SharedCube;
#[derive(Component)]
struct PresentationCamera;
#[derive(Component)]
struct Status;
#[derive(Component)]
struct Cursor(u64);
#[derive(Component)]
struct Overlay;
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.7,
            pitch: 0.5,
        }
    }
}
struct CubeAdapter;
impl SharedAdapter for CubeAdapter {
    type Document = Cube;
    const ID: &'static str = super::common::CubeState::ID;
    const VERSION: u32 = super::common::CubeState::VERSION;
    const POLICY: WritePolicy = WritePolicy::Editors;
    const CHANNEL: SharedChannel = SharedChannel::Document;
    fn install(app: &mut App) {
        app.add_systems(Update, apply_cube.after(CollaborationSystems::Transport));
    }
}
struct CameraAdapter;
impl SharedAdapter for CameraAdapter {
    type Document = Cube;
    const ID: &'static str = CAMERA_ID;
    const VERSION: u32 = CAMERA_VERSION;
    const POLICY: WritePolicy = WritePolicy::Presenter;
    const CHANNEL: SharedChannel = SharedChannel::PresentationCamera;
    fn install(app: &mut App) {
        app.init_resource::<Orbit>().add_systems(
            Update,
            camera
                .after(CollaborationSystems::Transport)
                .after(super::controls),
        );
    }
}
pub struct CubeViewPlugin;
impl Plugin for CubeViewPlugin {
    fn build(&self, app: &mut App) {
        assert!(
            app.register_shared::<CubeAdapter>().is_ok(),
            "invalid cube registration"
        );
        assert!(
            app.register_shared::<CameraAdapter>().is_ok(),
            "invalid camera registration"
        );
        #[cfg(feature = "example-dimensify")]
        app.add_plugins(dimensify::DimensifyPlugins {
            infinite_grid: false,
            with_sun: false,
            with_ambient_light: false,
            with_ui: false,
        });
        app.add_systems(PostStartup, setup)
            .add_systems(Update, presence.after(camera));
    }
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    #[cfg(feature = "example-dimensify")] cameras: Query<
        Entity,
        With<dimensify::camera::main_camera::MainCamera>,
    >,
) {
    commands.spawn((
        SharedCube,
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(materials.add(StandardMaterial::default())),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(8., 8., 0.1))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.15, 0.20),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0., 0., -0.8),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.,
            ..default()
        },
        Transform::from_xyz(3., -4., 6.).looking_at(Vec3::ZERO, Vec3::Z),
    ));
    // Keep this tiny example directly in the wire's right-handed Z-up frame.
    #[cfg(not(feature = "example-dimensify"))]
    let camera = commands.spawn(Camera3d::default()).id();
    #[cfg(feature = "example-dimensify")]
    let camera = cameras
        .single()
        .expect("Dimensify supplies the main camera");
    commands.entity(camera).insert((
        PresentationCamera,
        Msaa::Off,
        Transform::from_xyz(4., 4., 3.).looking_at(Vec3::ZERO, Vec3::Z),
    ));
    commands
        .spawn((
            Overlay,
            UiTargetCamera(camera),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                Status,
                Text::new("Connecting..."),
                TextFont::from_font_size(FontSize::Px(16.)),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    top: px(12),
                    ..default()
                },
            ));
        });
}
fn apply_cube(
    session: Res<ClientSession<Cube>>,
    mut cube: Query<(&mut Transform, &MeshMaterial3d<StandardMaterial>), With<SharedCube>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(room) = &session.snapshot else {
        return;
    };
    let Ok((mut transform, material)) = cube.single_mut() else {
        return;
    };
    transform.translation = Vec3::from_array(room.document.data.center);
    if let Some(mut material) = materials.get_mut(&material.0) {
        let [r, g, b] = room.document.data.color;
        material.base_color = Color::srgb(r, g, b);
    }
}
fn camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut session: ResMut<ClientSession<Cube>>,
    mut demo: ResMut<Demo>,
    mut orbit: ResMut<Orbit>,
    mut camera: Query<(&mut Transform, &mut Projection), With<PresentationCamera>>,
) {
    let Some(room) = &session.snapshot else {
        return;
    };
    let presenting = room.presenter == session.you;
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let horizontal =
        f32::from(keys.pressed(KeyCode::ArrowRight)) - f32::from(keys.pressed(KeyCode::ArrowLeft));
    let vertical =
        f32::from(keys.pressed(KeyCode::ArrowUp)) - f32::from(keys.pressed(KeyCode::ArrowDown));
    if horizontal != 0. || vertical != 0. {
        // Derive local orbit from the displayed pose when detaching after following.
        let eye = transform.translation;
        orbit.yaw = eye.y.atan2(eye.x) + horizontal * time.delta_secs();
        orbit.pitch =
            ((eye.z / eye.length()).asin() + vertical * time.delta_secs()).clamp(-1.3, 1.3);
        demo.following = false;
    }
    if !presenting && (demo.following || room.forced_follow) {
        if let Some(sample) = &room.camera {
            transform.translation = Vec3::from_array(sample.pose.position);
            transform.rotation = Quat::from_array(sample.pose.rotation);
            if let (
                Projection::Perspective(p),
                dimensify_session::Projection::Perspective { vertical_fov },
            ) = (&mut *projection, sample.pose.projection)
            {
                p.fov = vertical_fov;
            }
        }
        return;
    }
    if horizontal != 0. || vertical != 0. {
        let radius = 6.;
        *transform = Transform::from_xyz(
            radius * orbit.pitch.cos() * orbit.yaw.cos(),
            radius * orbit.pitch.cos() * orbit.yaw.sin(),
            radius * orbit.pitch.sin(),
        )
        .looking_at(Vec3::ZERO, Vec3::Z);
    }
    if !presenting {
        return;
    }
    let fov = match &*projection {
        Projection::Perspective(p) => p.fov,
        _ => return,
    };
    let pose = CameraPose {
        position: transform.translation.to_array(),
        rotation: transform.rotation.to_array(),
        projection: dimensify_session::Projection::Perspective { vertical_fov: fov },
        focus_distance: transform.translation.length(),
    };
    if let Err(error) = session.publish_camera_sample(pose, time.elapsed()) {
        eprintln!("Camera: {error}");
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Bevy injects each independent system parameter."
)]
fn presence(
    mut commands: Commands,
    window: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    mut next: Local<f32>,
    mut session: ResMut<ClientSession<Cube>>,
    demo: Res<Demo>,
    overlay: Query<Entity, With<Overlay>>,
    mut cursors: Query<(Entity, &Cursor, &mut Node, &mut Text), Without<Status>>,
    mut status: Query<&mut Text, With<Status>>,
) {
    let (Ok(window), Ok(overlay)) = (window.single(), overlay.single()) else {
        return;
    };
    let Some(room) = &session.snapshot else {
        if let Ok(mut status) = status.single_mut() {
            status.0 = format!("{:?} - {:?}", session.status, session.error);
        }
        for (entity, ..) in &mut cursors {
            commands.entity(entity).despawn();
        }
        return;
    };
    let presenting = room.presenter == session.you;
    let shared_view =
        presenting || ((demo.following || room.forced_follow) && room.camera.is_some());
    if let Ok(mut status) = status.single_mut() {
        let me = room.participants.iter().find(|p| Some(p.id) == session.you);
        status.0 = format!(
            "{:?} | {:?} | {} | revision {}\nSPACE: color / X: move (editors) | arrows: orbit locally | F: follow\nP: request presenter | G: grant first request | R: claim absent presenter\nParticipants: {} | Requests: {} | Error: {:?}",
            session.status,
            me.map(|p| p.role),
            if presenting {
                "Presenting"
            } else if shared_view {
                "Following"
            } else {
                "Exploring"
            },
            room.document.revision,
            room.participants.len(),
            room.presenter_requests.len(),
            session.error
        );
    }
    let desired: Vec<_> = if shared_view {
        session
            .remote_cursors()
            .filter_map(|cursor| {
                if !cursor.sample.matches(room) {
                    return None;
                }
                let [x, y] = cursor.sample.position?;
                let name = &room
                    .participants
                    .iter()
                    .find(|p| p.id == cursor.participant)?
                    .name;
                Some((
                    cursor.participant,
                    window.width() * 0.5 + x * window.height(),
                    window.height() * 0.5 + y * window.height(),
                    format!("< {name}"),
                ))
            })
            .collect()
    } else {
        vec![]
    };
    for (entity, cursor, mut node, mut text) in &mut cursors {
        match desired.iter().find(|(id, ..)| *id == cursor.0) {
            Some((_, x, y, label)) => {
                node.left = px(*x);
                node.top = px(*y);
                text.0.clone_from(label);
            }
            None => {
                commands.entity(entity).despawn();
            }
        }
    }
    for (id, x, y, label) in desired {
        if cursors.iter().any(|(_, cursor, ..)| cursor.0 == id) {
            continue;
        }
        commands.spawn((
            Cursor(id),
            ChildOf(overlay),
            Text::new(label),
            TextFont::from_font_size(FontSize::Px(14.)),
            TextColor(Color::srgb(1., 0.8, 0.3)),
            Node {
                position_type: PositionType::Absolute,
                left: px(x),
                top: px(y),
                ..default()
            },
        ));
    }
    // Calling again also retries a subscription that could not enter a full queue.
    let _ = session.set_cursor_sharing(shared_view);
    if !shared_view || time.elapsed_secs() < *next {
        return;
    }
    *next = time.elapsed_secs() + 0.05;
    let pointer = window
        .cursor_position()
        .filter(|_| window.focused && window.height() > 0.)
        .map(|p| {
            [
                (p.x - window.width() * 0.5) / window.height(),
                (p.y - window.height() * 0.5) / window.height(),
            ]
        });
    // Subscription acknowledgement can still be in flight; the next sample retries.
    let _ = session.publish_cursor(pointer);
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use dimensify_session::{ClientRequest, ServerMessage, authority::Authority};
    #[test]
    fn viewer_orbit_stays_local_and_follow_restores_presenter_pose() {
        let mut authority = Authority::<Cube>::default();
        let created = authority
            .process(
                1,
                ClientRequest::Create {
                    name: "host".into(),
                },
                0,
            )
            .unwrap();
        let ServerMessage::Welcome {
            snapshot,
            invitations: Some(invites),
            ..
        } = created[0].1.clone()
        else {
            panic!()
        };
        authority
            .process(
                1,
                ClientRequest::PublishCamera(CameraSnapshot {
                    room: snapshot.room.clone(),
                    epoch: 1,
                    sequence: 1,
                    scene_revision: 0,
                    pose: CameraPose {
                        position: [4., 4., 3.],
                        rotation: [0., 0., 0., 1.],
                        projection: dimensify_session::Projection::Perspective { vertical_fov: 1. },
                        focus_distance: 6.,
                    },
                }),
                0,
            )
            .unwrap();
        let mut session = ClientSession::<Cube>::default();
        for (_, msg) in authority
            .process(
                2,
                ClientRequest::Join {
                    room: snapshot.room,
                    invitation: invites.viewer,
                    name: "viewer".into(),
                },
                0,
            )
            .unwrap()
            .into_iter()
            .filter(|(id, _)| *id == 2)
        {
            session.receive(msg);
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(session)
            .insert_resource(Demo {
                following: true,
                ..default()
            })
            .init_resource::<Orbit>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, (super::super::controls, camera).chain());
        let entity = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                PresentationCamera,
                Transform::default(),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Transform>(entity).unwrap().translation,
            Vec3::new(4., 4., 3.)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowLeft);
        app.update();
        assert!(!app.world().resource::<Demo>().following);
        assert_eq!(
            app.world()
                .resource::<ClientSession<Cube>>()
                .pending_count(),
            0
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyF);
        app.update();
        assert!(app.world().resource::<Demo>().following);
        assert_eq!(
            app.world().get::<Transform>(entity).unwrap().translation,
            Vec3::new(4., 4., 3.)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.update();
        assert_eq!(
            app.world()
                .resource::<ClientSession<Cube>>()
                .pending_count(),
            0,
            "viewer edit input must stay local"
        );
    }
}
