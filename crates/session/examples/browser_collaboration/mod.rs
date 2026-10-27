//! Example-owned composition; transport/session authority stays in shared crates.
#[path = "../common/mod.rs"]
mod common;
mod connection;
mod connection_ui;
mod ui;
use bevy::prelude::*;
use common::{Cube, CubeState};
use dimensify::playground::{
    CameraPlaygroundPlugin, app,
    navigation::{
        CameraPose as RenderPose,
        viewer::{NavigationConfig, NavigationSystems, ViewerCamera},
    },
};
use dimensify_session::{
    CameraPose, ClientRequest, ClientSession, CollaborationClientPlugin, CollaborationSystems,
    InviteCode, Projection as WireProjection,
};

#[derive(Resource)]
struct Options {
    endpoint: String,
    code: String,
    name: String,
}
#[derive(Resource, Default)]
struct Demo {
    exploring: bool,
    error: String,
}
#[derive(Component)]
struct SharedCube;

/// Run the shared camera shell with a real room client and a canonical cube.
pub(super) fn run() {
    let (endpoint, code, name) = arguments();
    let mut app = app("Dimensify browser collaboration");
    connection_ui::configure(&mut app);
    app.insert_resource(Options {
        endpoint,
        code,
        name,
    })
    .init_resource::<connection::Panel>()
    .init_resource::<Demo>()
    .add_plugins(CollaborationClientPlugin::<Cube>::default())
    .add_plugins(CameraPlaygroundPlugin { scene: false })
    .add_message::<ui::Action>()
    .add_message::<connection_ui::Action>()
    .add_systems(Startup, (setup, connection::startup))
    .add_systems(PostStartup, (ui::setup, connection_ui::setup))
    .add_systems(
        Update,
        (
            ui::actions,
            connection_ui::update,
            connection::apply,
            follow,
        )
            .chain()
            .after(CollaborationSystems::Transport)
            .before(NavigationSystems),
    )
    .add_systems(
        Update,
        (publish, apply_cube, ui::status)
            .chain()
            .after(NavigationSystems),
    )
    .run();
}
fn arguments() -> (String, String, String) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let args: Vec<_> = std::env::args().skip(1).collect();
        (
            args.first()
                .cloned()
                .unwrap_or_else(|| "ws://127.0.0.1:5002".into()),
            args.get(1).cloned().unwrap_or_default(),
            args.get(2).cloned().unwrap_or_else(|| "participant".into()),
        )
    }
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()
            .unwrap()
            .location()
            .search()
            .unwrap_or_default();
        let values: std::collections::HashMap<_, _> =
            url::form_urlencoded::parse(search.trim_start_matches('?').as_bytes())
                .into_owned()
                .collect();
        (
            values
                .get("endpoint")
                .cloned()
                .unwrap_or_else(|| "ws://127.0.0.1:5002".into()),
            values.get("code").cloned().unwrap_or_default(),
            values
                .get("name")
                .cloned()
                .unwrap_or_else(|| "browser".into()),
        )
    }
}
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        SharedCube,
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.7, 0.9))),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(12., 0.1, 12.))),
        MeshMaterial3d(materials.add(Color::srgb(0.12, 0.16, 0.22))),
        Transform::from_xyz(0., -0.6, 0.),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.,
            ..default()
        },
        Transform::from_xyz(4., 6., 5.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
fn basis() -> Quat {
    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}
fn follow(
    session: Res<ClientSession<Cube>>,
    demo: Res<Demo>,
    mut config: ResMut<NavigationConfig>,
) {
    config.override_pose = None;
    let Some(room) = &session.snapshot else {
        return;
    };
    if room.presenter == session.you || (demo.exploring && !room.forced_follow) {
        return;
    }
    if let Some(sample) = &room.camera {
        let WireProjection::Perspective { vertical_fov } = sample.pose.projection else {
            return;
        };
        config.override_pose = Some(RenderPose::from_rotation(
            basis() * Vec3::from_array(sample.pose.position),
            basis() * Quat::from_array(sample.pose.rotation),
            sample.pose.focus_distance,
            vertical_fov,
        ));
    }
}
fn publish(
    time: Res<Time>,
    cameras: Query<(&Transform, &Projection), With<ViewerCamera>>,
    mut session: ResMut<ClientSession<Cube>>,
) {
    let Some(room) = &session.snapshot else {
        return;
    };
    if room.presenter != session.you {
        return;
    }
    let Ok((transform, Projection::Perspective(projection))) = cameras.single() else {
        return;
    };
    let _ = session.publish_camera_sample(
        CameraPose {
            position: (basis().inverse() * transform.translation).to_array(),
            rotation: (basis().inverse() * transform.rotation).to_array(),
            focus_distance: transform.translation.length().max(0.01),
            projection: WireProjection::Perspective {
                vertical_fov: projection.fov,
            },
        },
        time.elapsed(),
    );
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
    transform.translation = basis() * Vec3::from_array(room.document.data.center);
    if let Some(mut material) = materials.get_mut(&material.0) {
        let [r, g, b] = room.document.data.color;
        material.base_color = Color::srgb(r, g, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_z_up_positions_and_rotations_round_trip_once() {
        let point = Vec3::new(1., 2., 3.);
        assert!((basis() * Vec3::Z).abs_diff_eq(Vec3::Y, 1e-6));
        assert!((basis().inverse() * (basis() * point)).abs_diff_eq(point, 1e-6));
        let rotation = Quat::from_euler(EulerRot::XYZ, 0.2, 0.4, 0.7);
        assert!((basis().inverse() * (basis() * rotation)).abs_diff_eq(rotation, 1e-6));
    }
}
