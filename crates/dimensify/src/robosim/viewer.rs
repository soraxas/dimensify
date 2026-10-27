use super::{RobotModel, RobotPlugin};
use bevy::{
    app::{AppExit, ScheduleRunnerPlugin},
    camera::{RenderTarget, primitives::MeshAabb},
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::{ExitCondition, PrimaryWindow, RequestRedraw},
    winit::WinitPlugin,
};
use dimensify_camera::{
    CameraPose, FitExtent, InspectContext, InspectController, InspectDrag, InspectFrame,
    OrbitState, PoseTransition, TargetPose, inspect_wheel_input, pointer_drag_owned,
    scene_bounding_sphere,
};
use eyre::{Result, ensure};
use std::{path::PathBuf, time::Duration};

#[derive(Resource, Default)]
struct Controls {
    selected: usize,
}

#[derive(Component)]
struct Orbit {
    pose: CameraPose,
    view: OrbitState,
    target: TargetPose,
    transition: PoseTransition,
    inspect: InspectController,
    drag: InspectDrag,
    left: bool,
    right: bool,
    framed: bool,
    snap: bool,
}

impl Default for Orbit {
    fn default() -> Self {
        // Preserve the robot viewer's initial elevated view and framing direction.
        let rotation = Quat::from_rotation_y(0.65) * Quat::from_rotation_x(-0.35);
        let pose = CameraPose::from_rotation(
            Vec3::Y * 0.7 + rotation * Vec3::Z * 4.0,
            rotation,
            4.0,
            PerspectiveProjection::default().fov,
        );
        let mut view = OrbitState::default();
        view.publish_pose(&pose);
        Self {
            pose,
            view,
            target: TargetPose(pose),
            transition: default(),
            inspect: default(),
            drag: default(),
            left: false,
            right: false,
            framed: false,
            snap: false,
        }
    }
}

#[derive(Resource, Default)]
struct NavigationProjection(Projection);

struct NavigationPlugin;

impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavigationProjection>().add_systems(
            PostUpdate,
            (frame_robot, orbit_camera)
                .chain()
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::CameraUpdateSystems),
        );
    }
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
        .add_plugins((RobotPlugin, NavigationPlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (joint_controls, update_help).chain());
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
            Orbit::default().pose.to_transform(),
            Orbit::default(),
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
        "{}\n\nTab: select joint    Left / Right: move    R: reset\nLeft drag: orbit    Right drag: pan    Wheel: zoom    F: frame robot    Esc: exit\n\n{selected}",
        model.robot.name
    );
    if text.0 != label {
        text.0 = label;
    }
}

#[allow(clippy::too_many_arguments)]
fn orbit_camera(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut scratch: ResMut<NavigationProjection>,
    mut redraw: MessageWriter<RequestRedraw>,
    mut camera: Single<(
        &mut Orbit,
        &Camera,
        &Projection,
        &mut Transform,
        &mut GlobalTransform,
    )>,
) {
    let (state, camera, projection, transform, global) = &mut *camera;
    let focused = window.as_ref().is_some_and(|window| window.focused);
    let pointer = window.as_ref().and_then(|window| window.cursor_position());
    let allowed = focused && !state.snap;
    let start_allowed = allowed
        && pointer.is_some_and(|point| {
            camera
                .logical_viewport_rect()
                .is_some_and(|rect| rect.contains(point))
        });
    state.left = allowed
        && pointer_drag_owned(
            state.left,
            buttons.pressed(MouseButton::Left),
            buttons.just_pressed(MouseButton::Left),
            buttons.just_released(MouseButton::Left),
            start_allowed,
        );
    state.right = allowed
        && pointer_drag_owned(
            state.right,
            buttons.pressed(MouseButton::Right),
            buttons.just_pressed(MouseButton::Right),
            buttons.just_released(MouseButton::Right),
            start_allowed,
        );
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let left = state.left;
    let right = state.right;
    let gesture = state.drag.update_owned(left, right, shift);
    let zoom = scroll.delta.y
        * if scroll.unit == MouseScrollUnit::Pixel {
            0.01
        } else {
            1.0
        };
    let mut input = if start_allowed {
        inspect_wheel_input(pointer, Vec2::ZERO, Vec2::ZERO, zoom, false)
    } else {
        default()
    };
    if gesture.is_some() {
        input.gesture = gesture;
        input.pointer = pointer;
        input.delta = motion.delta / window.as_ref().map_or(1.0, |window| window.scale_factor());
    }
    let Orbit {
        pose,
        view,
        target,
        transition,
        inspect,
        snap,
        ..
    } = &mut **state;
    inspect.advance(
        &InspectFrame {
            input,
            motion_suppressed: !allowed,
            collision_enabled: false,
            delta_time: time.delta(),
            ..default()
        },
        InspectContext {
            camera,
            projection,
            scratch: scratch.reborrow().map_unchanged(|value| &mut value.0),
            live: pose,
            orbit: view,
            target,
            transition,
        },
        &mut redraw,
        &(),
    );
    if std::mem::take(snap) {
        transition.timer = 1.0;
    }
    *pose = transition.advance(&target.0, time.delta_secs());
    let next = pose.to_transform();
    if **transform != next {
        **transform = next;
    }
    // The robot camera is unparented. Publish the final pose in this same frame,
    // after geometry propagation and before camera/frustum extraction.
    let next_global = GlobalTransform::from(next);
    if **global != next_global {
        **global = next_global;
    }
}

fn frame_robot(
    keys: Res<ButtonInput<KeyCode>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    meshes: Res<Assets<Mesh>>,
    geometry: Query<(&Mesh3d, &GlobalTransform)>,
    mut camera: Single<&mut Orbit>,
) {
    let focused = window.as_ref().is_some_and(|window| window.focused);
    if camera.framed && !(focused && keys.just_pressed(KeyCode::KeyF)) {
        return;
    }
    let extents: Vec<_> = geometry
        .iter()
        .filter_map(|(handle, world)| {
            let bounds = meshes.get(&handle.0)?.compute_aabb()?;
            Some((
                FitExtent::from_center_half_extents(
                    bounds.center.into(),
                    bounds.half_extents.into(),
                ),
                world,
            ))
        })
        .collect();
    if let Some((center, radius)) =
        scene_bounding_sphere(extents.iter().map(|(extent, world)| (extent, *world)))
    {
        camera.view.target_focus = center;
        // Keep the established diagonal * 1.8 framing margin.
        camera.view.target_distance = (radius * 3.6).max(0.5);
        camera.view.request_view();
        camera.framed = true;
        camera.snap = true;
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

#[cfg(test)]
#[path = "camera_tests.rs"]
mod camera_tests;
