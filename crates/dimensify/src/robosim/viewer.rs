use super::{RobotModel, RobotPlugin};
use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    camera::{RenderTarget, primitives::MeshAabb},
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use eyre::{Result, ensure};
use std::{path::PathBuf, time::Duration};

#[derive(Resource, Default)]
struct Controls {
    selected: usize,
}

#[derive(Component)]
struct Orbit {
    focus: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
    framed: bool,
}

#[derive(Component)]
struct HelpText;

#[derive(Resource)]
struct Capture {
    path: PathBuf,
    image: Option<Handle<Image>>,
    frames: u32,
}

/// Configure the native window or offscreen renderer around the shared robot plugin.
pub(super) fn run(model: RobotModel, screenshot: Option<PathBuf>) -> Result<()> {
    let mut app = App::new();
    let offscreen = screenshot.is_some();
    let plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: if offscreen {
            None
        } else {
            Some(Window {
                title: "RoboSim".into(),
                resolution: (1100, 800).into(),
                ..default()
            })
        },
        exit_condition: if offscreen {
            ExitCondition::DontExit
        } else {
            ExitCondition::OnAllClosed
        },
        ..default()
    });
    if let Some(path) = screenshot {
        app.add_plugins(plugins.disable::<WinitPlugin>())
            .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 30.0,
            )))
            .insert_resource(Capture {
                path,
                image: None,
                frames: 0,
            })
            .add_systems(Update, capture_frame);
    } else {
        app.add_plugins(plugins);
    }
    app.insert_resource(model)
        .insert_resource(ClearColor(Color::srgb(0.055, 0.075, 0.10)))
        .init_resource::<Controls>()
        .add_plugins(RobotPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, (joint_controls, update_help).chain())
        .add_systems(
            PostUpdate,
            frame_robot.after(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(Update, orbit_camera);
    ensure!(
        app.run() == AppExit::Success,
        "RoboSim exited with an error"
    );
    Ok(())
}

fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    capture: Option<ResMut<Capture>>,
) {
    let camera = commands
        .spawn((
            Camera3d::default(),
            Transform::from_xyz(2.8, 2.2, 3.5).looking_at(Vec3::Y * 0.7, Vec3::Y),
            Orbit {
                focus: Vec3::Y * 0.7,
                distance: 4.0,
                yaw: 0.65,
                pitch: 0.35,
                framed: false,
            },
        ))
        .id();
    if let Some(mut capture) = capture {
        let image = images.add(Image::new_target_texture(
            1100,
            800,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        commands
            .entity(camera)
            .insert(RenderTarget::Image(image.clone().into()));
        capture.image = Some(image);
    }
    commands.spawn((
        DirectionalLight {
            illuminance: 14000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 6.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 5000.0,
            ..default()
        },
        Transform::from_xyz(-3.0, 2.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        HelpText,
        Text::default(),
        TextFont::from_font_size(FontSize::Px(17.0)),
        TextColor(Color::WHITE),
        UiTargetCamera(camera),
        Node {
            position_type: PositionType::Absolute,
            top: px(18),
            left: px(20),
            ..default()
        },
    ));
}

fn joint_controls(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut controls: ResMut<Controls>,
    mut model: ResMut<RobotModel>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    let indices: Vec<_> = model.controllable_joints().map(|(i, _)| i).collect();
    if indices.is_empty() {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        controls.selected = (controls.selected + 1) % indices.len();
    }
    let index = indices[controls.selected];
    let direction =
        i32::from(keys.pressed(KeyCode::ArrowRight)) - i32::from(keys.pressed(KeyCode::ArrowLeft));
    if direction != 0 {
        let value = model.joint_position(index) + direction as f32 * time.delta_secs();
        // Input and the model's validated URDF guarantee a finite, controllable joint.
        if let Err(error) = model.set_joint_position(index, value) {
            warn!("{error}");
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        for index in indices {
            let _ = model.set_joint_position(index, 0.0);
        }
    }
}

fn update_help(
    model: Res<RobotModel>,
    controls: Res<Controls>,
    mut text: Single<&mut Text, With<HelpText>>,
) {
    let selected = model
        .controllable_joints()
        .nth(controls.selected)
        .map(|(index, name)| {
            let unit = if model.robot.joints[index].joint_type == urdf_rs::JointType::Prismatic {
                "m"
            } else {
                "rad"
            };
            format!("{name}: {:.3} {unit}", model.joint_position(index))
        })
        .unwrap_or_else(|| "No movable joints".into());
    let label = format!(
        "{}\n\nTab: select joint    Left / Right: move    R: reset\nDrag: orbit    Wheel: zoom    F: frame robot    Esc: exit\n\n{selected}",
        model.robot.name
    );
    if text.0 != label {
        text.0 = label;
    }
}

fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut camera: Single<(&mut Orbit, &mut Transform)>,
) {
    let (orbit, transform) = &mut *camera;
    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.006;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.006).clamp(-1.45, 1.45);
    }
    orbit.distance = (orbit.distance * (-scroll.delta.y * 0.1).exp()).clamp(0.05, 10000.0);
    **transform = camera_transform(orbit);
}

fn camera_transform(orbit: &Orbit) -> Transform {
    let direction = Vec3::new(
        orbit.yaw.sin() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.cos() * orbit.pitch.cos(),
    );
    Transform::from_translation(orbit.focus + direction * orbit.distance)
        .looking_at(orbit.focus, Vec3::Y)
}

fn frame_robot(
    keys: Res<ButtonInput<KeyCode>>,
    meshes: Res<Assets<Mesh>>,
    geometry: Query<(&Mesh3d, &GlobalTransform)>,
    mut camera: Single<(&mut Orbit, &mut Transform)>,
) {
    let (orbit, transform) = &mut *camera;
    if orbit.framed && !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (handle, world) in &geometry {
        let Some(bounds) = meshes.get(&handle.0).and_then(Mesh::compute_aabb) else {
            continue;
        };
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let corner = Vec3::from(bounds.center)
                        + Vec3::from(bounds.half_extents) * Vec3::new(x, y, z);
                    let point = world.transform_point(corner);
                    min = min.min(point);
                    max = max.max(point);
                }
            }
        }
    }
    if min.is_finite() && max.is_finite() {
        orbit.focus = (min + max) * 0.5;
        orbit.distance = ((max - min).length() * 1.8).max(0.5);
        orbit.framed = true;
        **transform = camera_transform(orbit);
    }
}

fn capture_frame(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    capture.frames += 1;
    if time.elapsed_secs() > 60.0 {
        error!("Offscreen capture timed out");
        exit.write(AppExit::error());
    }
    if capture.frames == 90 {
        let Some(image) = capture.image.clone() else {
            return;
        };
        let path = capture.path.clone();
        commands.spawn(Screenshot::image(image)).observe(
            move |event: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                let result = event
                    .image
                    .clone()
                    .try_into_dynamic()
                    .map_err(|e| e.to_string())
                    .and_then(|image| image.save(&path).map_err(|e| e.to_string()));
                match result {
                    Ok(()) => {
                        println!("Saved {}", path.display());
                        exit.write(AppExit::Success);
                    }
                    Err(error) => {
                        error!("Screenshot failed: {error}");
                        exit.write(AppExit::error());
                    }
                }
            },
        );
    }
}
