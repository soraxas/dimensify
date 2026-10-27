use super::*;
use bevy::{
    camera::{RenderTargetInfo, Viewport},
    picking::hover::Hovered,
    window::{PrimaryWindow, RequestRedraw},
};
use std::time::Duration;

fn pose_matches(actual: &Transform, expected: &Transform) -> bool {
    actual.translation.abs_diff_eq(expected.translation, 1e-6)
        && actual.rotation.abs_diff_eq(expected.rotation, 1e-6)
}

fn fixture() -> (App, Entity, Entity, Entity) {
    let orbit = OrbitState {
        yaw: 0.5,
        target_yaw: 0.5,
        pitch: -0.3,
        target_pitch: -0.3,
        distance: 6.0,
        target_distance: 6.0,
        ..default()
    };
    let pose = orbit.requested_pose();
    let projection = Projection::Perspective(PerspectiveProjection {
        aspect_ratio: 1.0,
        fov: pose.fov,
        ..default()
    });
    let mut camera = Camera {
        viewport: Some(Viewport {
            physical_size: UVec2::splat(1000),
            ..default()
        }),
        ..default()
    };
    camera.computed.target_info = Some(RenderTargetInfo {
        physical_size: UVec2::splat(1000),
        scale_factor: 1.0,
    });
    camera.computed.clip_from_view = projection.get_clip_from_view();
    let mut app = App::new();
    app.init_resource::<NavigationProjection>()
        .insert_resource(NavigationMode::Orbit)
        .add_message::<CameraAction>()
        .insert_resource(orbit)
        .init_resource::<NavigationConfig>()
        .init_resource::<Time>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<bevy::input::touch::TouchInput>()
        .init_resource::<bevy::input::touch::Touches>()
        .add_systems(PreUpdate, bevy::input::touch::touch_screen_input_system)
        .add_message::<MouseMotion>()
        .add_message::<MouseWheel>()
        .add_message::<RequestRedraw>()
        .add_systems(Update, navigate);
    let camera = app
        .world_mut()
        .spawn((ViewerCamera, camera, projection, pose.to_transform()))
        .id();
    let mut window = Window {
        resolution: (1000, 1000).into(),
        ..default()
    };
    window.set_cursor_position(Some(Vec2::splat(500.0)));
    let window = app.world_mut().spawn((PrimaryWindow, window)).id();
    let ui = app.world_mut().spawn(Hovered(false)).id();
    (app, camera, window, ui)
}

fn update(app: &mut App, delta: Vec2) {
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(1.0 / 60.0));
    app.world_mut().write_message(MouseMotion { delta });
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
}

#[test]
fn a_drag_starting_on_ui_cannot_take_the_camera_when_it_leaves_ui() {
    let (mut app, camera, _, ui) = fixture();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut().entity_mut(ui).insert(Hovered(true));
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    update(&mut app, Vec2::new(40.0, 10.0));
    app.world_mut().entity_mut(ui).insert(Hovered(false));
    update(&mut app, Vec2::new(40.0, 10.0));
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &initial
    ));
}

#[test]
fn losing_window_focus_cancels_camera_motion() {
    let (mut app, camera, window, _) = fixture();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    update(&mut app, Vec2::new(40.0, 10.0));
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &initial
    ));
}

#[test]
fn released_drag_delivers_its_smoothing_tail() {
    let (mut app, camera, _, _) = fixture();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    update(&mut app, Vec2::new(80.0, 20.0));
    let first = app.world().get::<Transform>(camera).unwrap().rotation;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    std::thread::sleep(Duration::from_millis(45));
    update(&mut app, Vec2::ZERO);
    let drained = app.world().get::<Transform>(camera).unwrap().rotation;
    assert!(
        !first.abs_diff_eq(drained, 1e-4),
        "release must deliver buffered displacement"
    );
    update(&mut app, Vec2::ZERO);
    assert!(
        app.world()
            .get::<Transform>(camera)
            .unwrap()
            .rotation
            .abs_diff_eq(drained, 1e-6)
    );
}

#[test]
fn file_load_frames_with_the_existing_margin_and_uses_the_shared_pose() {
    let (mut app, camera, _, _) = fixture();
    let bounds = (Vec3::NEG_ONE, Vec3::ONE);
    app.world_mut().resource_mut::<NavigationConfig>().bounds = Some(bounds);
    app.world_mut().write_message(CameraAction::Frame);
    update(&mut app, Vec2::ZERO);
    let orbit = app.world().resource::<OrbitState>();
    let expected = perspective_fit_distance(3.0_f32.sqrt() * (1.2 / 1.05), orbit.fov, Vec2::ONE);
    assert!((orbit.distance - expected).abs() < 1e-5);
    assert!(
        app.world()
            .get::<Transform>(camera)
            .unwrap()
            .translation
            .abs_diff_eq(orbit.requested_pose().position, 1e-5)
    );
}

#[test]
fn signed_shift_wheel_pans_in_the_application_direction() {
    let (mut app, camera, window, _) = fixture();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ShiftLeft);
    app.world_mut().write_message(MouseWheel {
        phase: bevy::input::touch::TouchPhase::Moved,
        window,
        unit: MouseScrollUnit::Line,
        x: 1.0,
        y: 0.0,
    });
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    let after = *app.world().get::<Transform>(camera).unwrap();
    assert!(after.rotation.abs_diff_eq(initial.rotation, 1e-6));
    assert!((after.translation - initial.translation).dot(initial.rotation * Vec3::X) > 0.0);
}

#[test]
fn control_overrides_shift_and_uses_the_application_wheel_direction() {
    let (mut app, _, window, _) = fixture();
    let initial_yaw = app.world().resource::<OrbitState>().yaw;
    for key in [KeyCode::ControlLeft, KeyCode::ShiftLeft] {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
    }
    app.world_mut().write_message(MouseWheel {
        phase: bevy::input::touch::TouchPhase::Moved,
        window,
        unit: MouseScrollUnit::Line,
        x: 1.0,
        y: 0.0,
    });
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    assert!(app.world().resource::<OrbitState>().yaw > initial_yaw);
}

#[test]
fn fly_movement_uses_shared_momentum_and_reset_clears_it() {
    let (mut app, camera, _, _) = fixture();
    app.world_mut().insert_resource(NavigationMode::Fly);
    let before = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    for _ in 0..12 {
        update(&mut app, Vec2::ZERO);
    }
    let moved = *app.world().get::<Transform>(camera).unwrap();
    assert!((moved.translation - before.translation).dot(before.rotation * Vec3::NEG_Z) > 0.1);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyW);
    app.world_mut().write_message(CameraAction::Reset);
    update(&mut app, Vec2::ZERO);
    assert_eq!(
        app.world().get::<Transform>(camera).unwrap().translation,
        Vec3::ZERO
    );
    for _ in 0..12 {
        update(&mut app, Vec2::ZERO);
    }
    assert_eq!(
        app.world().get::<Transform>(camera).unwrap().translation,
        Vec3::ZERO
    );
    assert!(
        app.world()
            .get::<Transform>(camera)
            .unwrap()
            .rotation
            .abs_diff_eq(Quat::IDENTITY, 1e-6)
    );
}

#[test]
fn switching_modes_preserves_the_visible_pose_and_ui_capture_stops_fly() {
    let (mut app, camera, _, ui) = fixture();
    let before = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut().write_message(CameraAction::ToggleMode);
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &before
    ));
    assert_eq!(
        *app.world().resource::<NavigationMode>(),
        NavigationMode::Fly
    );
    app.world_mut().entity_mut(ui).insert(Hovered(true));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyW);
    for _ in 0..6 {
        update(&mut app, Vec2::ZERO);
    }
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &before
    ));
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .release(KeyCode::KeyW);
    app.world_mut().write_message(CameraAction::ToggleMode);
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &before
    ));
}

fn touch(
    app: &mut App,
    window: Entity,
    id: u64,
    phase: bevy::input::touch::TouchPhase,
    position: Vec2,
) {
    app.world_mut()
        .write_message(bevy::input::touch::TouchInput {
            window,
            id,
            phase,
            position,
            force: None,
        });
}

#[test]
fn one_finger_moves_the_orbit_camera() {
    use bevy::input::touch::TouchPhase;
    let (mut app, camera, window, _) = fixture();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Started,
        Vec2::new(400.0, 500.0),
    );
    update(&mut app, Vec2::ZERO);
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Moved,
        Vec2::new(500.0, 530.0),
    );
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    assert!(
        !app.world()
            .get::<Transform>(camera)
            .unwrap()
            .rotation
            .abs_diff_eq(initial.rotation, 1e-4)
    );
}

#[test]
fn touch_ignores_a_mouse_cursor_left_hovering_over_ui() {
    use bevy::input::touch::TouchPhase;
    let (mut app, camera, window, ui) = fixture();
    app.init_resource::<bevy::picking::hover::HoverMap>();
    app.world_mut().entity_mut(ui).insert(Hovered(true));
    let initial = *app.world().get::<Transform>(camera).unwrap();
    touch(&mut app, window, 1, TouchPhase::Started, Vec2::splat(400.0));
    update(&mut app, Vec2::ZERO);
    touch(&mut app, window, 1, TouchPhase::Moved, Vec2::splat(500.0));
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    assert!(!pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &initial
    ));
}

#[test]
fn per_finger_ui_hit_blocks_touch_without_mouse_hover() {
    use bevy::{
        input::touch::TouchPhase,
        picking::{backend::HitData, hover::HoverMap, pointer::PointerId},
    };
    let (mut app, camera, window, ui) = fixture();
    app.init_resource::<HoverMap>();
    app.world_mut().entity_mut(ui).insert(Node::default());
    app.world_mut()
        .resource_mut::<HoverMap>()
        .entry(PointerId::Touch(1))
        .or_default()
        .insert(ui, HitData::new(camera, 0.0, None, None));
    let initial = *app.world().get::<Transform>(camera).unwrap();
    touch(&mut app, window, 1, TouchPhase::Started, Vec2::splat(400.0));
    update(&mut app, Vec2::ZERO);
    app.world_mut().resource_mut::<HoverMap>().clear();
    touch(&mut app, window, 1, TouchPhase::Moved, Vec2::splat(500.0));
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &initial
    ));
}

#[test]
fn two_fingers_pan_and_pinch_without_rotating() {
    use bevy::input::touch::TouchPhase;
    let (mut app, camera, window, _) = fixture();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Started,
        Vec2::new(400.0, 500.0),
    );
    touch(
        &mut app,
        window,
        2,
        TouchPhase::Started,
        Vec2::new(600.0, 500.0),
    );
    update(&mut app, Vec2::ZERO);
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Moved,
        Vec2::new(410.0, 480.0),
    );
    touch(
        &mut app,
        window,
        2,
        TouchPhase::Moved,
        Vec2::new(650.0, 520.0),
    );
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    let after = app.world().get::<Transform>(camera).unwrap();
    assert!(after.rotation.abs_diff_eq(initial.rotation, 1e-6));
    assert!(after.translation.distance(initial.translation) > 0.01);
    assert!(app.world().resource::<OrbitState>().distance < 6.0);
}

#[test]
fn touch_started_on_ui_cannot_become_scene_input_and_cancel_clears_motion() {
    use bevy::input::touch::TouchPhase;
    let (mut app, camera, window, ui) = fixture();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut().entity_mut(ui).insert(Hovered(true));
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Started,
        Vec2::new(400.0, 500.0),
    );
    update(&mut app, Vec2::ZERO);
    app.world_mut().entity_mut(ui).insert(Hovered(false));
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Moved,
        Vec2::new(500.0, 530.0),
    );
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &initial
    ));
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Ended,
        Vec2::new(500.0, 530.0),
    );
    update(&mut app, Vec2::ZERO);
    touch(
        &mut app,
        window,
        2,
        TouchPhase::Started,
        Vec2::new(400.0, 500.0),
    );
    update(&mut app, Vec2::ZERO);
    touch(
        &mut app,
        window,
        2,
        TouchPhase::Moved,
        Vec2::new(500.0, 530.0),
    );
    update(&mut app, Vec2::ZERO);
    let before_cancel = *app.world().get::<Transform>(camera).unwrap();
    touch(
        &mut app,
        window,
        2,
        TouchPhase::Canceled,
        Vec2::new(500.0, 530.0),
    );
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &before_cancel
    ));
}

#[test]
fn touch_look_works_in_fly_and_reset_retires_held_contacts() {
    use bevy::input::touch::TouchPhase;
    let (mut app, camera, window, _) = fixture();
    app.world_mut().insert_resource(NavigationMode::Fly);
    let initial = *app.world().get::<Transform>(camera).unwrap();
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Started,
        Vec2::new(400.0, 500.0),
    );
    update(&mut app, Vec2::ZERO);
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Moved,
        Vec2::new(500.0, 530.0),
    );
    update(&mut app, Vec2::ZERO);
    for _ in 0..8 {
        update(&mut app, Vec2::ZERO);
    }
    assert!(
        !app.world()
            .get::<Transform>(camera)
            .unwrap()
            .rotation
            .abs_diff_eq(initial.rotation, 1e-4)
    );
    app.world_mut().write_message(CameraAction::Reset);
    update(&mut app, Vec2::ZERO);
    touch(
        &mut app,
        window,
        1,
        TouchPhase::Moved,
        Vec2::new(600.0, 560.0),
    );
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &home_pose().to_transform()
    ));
}

#[test]
fn following_pose_does_not_replay_wheel_input_when_released() {
    let (mut app, camera, window, _) = fixture();
    let pose = CameraPose::from_rotation(Vec3::new(2., 3., 8.), Quat::IDENTITY, 8., 0.8);
    app.world_mut()
        .resource_mut::<NavigationConfig>()
        .override_pose = Some(pose);
    app.world_mut().write_message(MouseWheel {
        phase: bevy::input::touch::TouchPhase::Moved,
        window,
        unit: MouseScrollUnit::Line,
        x: 0.,
        y: 5.,
    });
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &pose.to_transform()
    ));
    app.world_mut()
        .resource_mut::<NavigationConfig>()
        .override_pose = None;
    update(&mut app, Vec2::ZERO);
    std::thread::sleep(Duration::from_millis(70));
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &pose.to_transform()
    ));
}

#[test]
fn reset_uses_the_consumers_home_pose() {
    let (mut app, camera, _, _) = fixture();
    let home = CameraPose::from_rotation(Vec3::new(2., 3., 8.), Quat::IDENTITY, 8., 0.8);
    app.world_mut().resource_mut::<NavigationConfig>().home = home;
    app.world_mut().write_message(CameraAction::Reset);
    update(&mut app, Vec2::ZERO);
    assert!(pose_matches(
        app.world().get::<Transform>(camera).unwrap(),
        &home.to_transform()
    ));
}
