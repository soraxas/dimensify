use super::*;
use bevy::{
    camera::{RenderTargetInfo, Viewport},
    window::{PrimaryWindow, RequestRedraw},
};

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(1.0 / 60.0));
    app.insert_resource(time)
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<AccumulatedMouseMotion>()
        .init_resource::<AccumulatedMouseScroll>()
        .add_message::<RequestRedraw>()
        .add_plugins((TransformPlugin, NavigationPlugin))
        .add_systems(Startup, setup);
    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    app.update();
    let camera = app
        .world_mut()
        .query_filtered::<Entity, With<Orbit>>()
        .single(app.world())
        .unwrap();
    let projection = app.world().get::<Projection>(camera).unwrap().clone();
    let mut view = app.world_mut().get_mut::<Camera>(camera).unwrap();
    view.viewport = Some(Viewport {
        physical_size: UVec2::splat(1000),
        ..default()
    });
    view.computed.target_info = Some(RenderTargetInfo {
        physical_size: UVec2::splat(1000),
        scale_factor: 1.0,
    });
    view.computed.clip_from_view = projection.get_clip_from_view();
    app.world_mut()
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::splat(500.0)));
    let mut state = app.world_mut().get_mut::<Orbit>(camera).unwrap();
    state.inspect.editor.smoothing.orbit = Duration::ZERO;
    state.inspect.editor.smoothing.pan = Duration::ZERO;
    state.inspect.editor.smoothing.zoom = Duration::ZERO;
    (app, camera, window)
}

#[test]
fn unfocused_robot_view_does_not_orbit_or_zoom() {
    let (mut app, camera, window) = fixture();
    let before = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(50.0, 20.0);
    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::Y;
    app.update();
    assert_eq!(*app.world().get::<Transform>(camera).unwrap(), before);
}

#[test]
fn right_drag_pans_the_robot_view_without_rotating_it() {
    let (mut app, camera, _) = fixture();
    let before = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Right);
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(50.0, 20.0);
    for _ in 0..3 {
        app.update();
    }
    let after = app.world().get::<Transform>(camera).unwrap();
    assert!(after.translation.distance(before.translation) > 0.001);
    assert!(after.rotation.dot(before.rotation).abs() > 0.99999);
}

#[test]
fn focus_restore_cannot_acquire_a_button_already_held() {
    let (mut app, camera, window) = fixture();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    let before = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(60.0, 20.0);
    app.update();
    assert_eq!(*app.world().get::<Transform>(camera).unwrap(), before);
}

#[test]
fn startup_and_explicit_frame_follow_propagated_geometry_without_stale_camera_pose() {
    let (mut app, camera, _) = fixture();
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(2.0, 4.0, 6.0));
    let part = app
        .world_mut()
        .spawn((Mesh3d(mesh), Transform::from_xyz(4.0, 5.0, 6.0)))
        .id();
    app.update();
    let distance = 3.6 * 14.0_f32.sqrt();
    let initial = *app.world().get::<Transform>(camera).unwrap();
    assert!(
        (initial.translation + *initial.forward() * distance - Vec3::new(4.0, 5.0, 6.0)).length()
            < 1e-4
    );
    assert_eq!(
        *app.world().get::<GlobalTransform>(camera).unwrap(),
        GlobalTransform::from(initial)
    );
    // Animation does not continuously reframe the view.
    app.world_mut()
        .get_mut::<Transform>(part)
        .unwrap()
        .translation
        .x = 14.0;
    app.update();
    assert_eq!(*app.world().get::<Transform>(camera).unwrap(), initial);
    // An explicit request sees the same frame's freshly propagated geometry.
    app.world_mut()
        .get_mut::<Transform>(part)
        .unwrap()
        .translation
        .x = 24.0;
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.update();
    let reframed = *app.world().get::<Transform>(camera).unwrap();
    assert!(
        (reframed.translation + *reframed.forward() * distance - Vec3::new(24.0, 5.0, 6.0))
            .length()
            < 1e-4
    );
    assert_eq!(
        *app.world().get::<GlobalTransform>(camera).unwrap(),
        GlobalTransform::from(reframed)
    );
}

#[test]
fn wheel_zoom_and_left_orbit_use_the_same_live_pose() {
    let (mut app, camera, _) = fixture();
    let before = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::Y;
    app.update();
    let zoomed = *app.world().get::<Transform>(camera).unwrap();
    assert!(zoomed.translation.distance(before.translation) > 0.001);
    assert!(zoomed.rotation.dot(before.rotation).abs() > 0.99999);
    app.world_mut()
        .resource_mut::<AccumulatedMouseScroll>()
        .delta = Vec2::ZERO;
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(50.0, 20.0);
    app.update();
    let orbited = app.world().get::<Transform>(camera).unwrap();
    assert!(orbited.rotation.dot(zoomed.rotation).abs() < 0.99999);
    assert_eq!(
        *app.world().get::<GlobalTransform>(camera).unwrap(),
        GlobalTransform::from(*orbited)
    );
}

#[test]
fn explicit_frame_interrupts_an_owned_drag() {
    let (mut app, camera, _) = fixture();
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(2.0, 4.0, 6.0));
    let part = app
        .world_mut()
        .spawn((Mesh3d(mesh), Transform::from_xyz(4.0, 5.0, 6.0)))
        .id();
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.world_mut()
        .resource_mut::<AccumulatedMouseMotion>()
        .delta = Vec2::new(50.0, 20.0);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .clear();
    app.world_mut()
        .get_mut::<Transform>(part)
        .unwrap()
        .translation = Vec3::new(24.0, 5.0, 6.0);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::KeyF);
    app.update();
    let view = app.world().get::<Transform>(camera).unwrap();
    assert!(
        (view.translation + *view.forward() * (3.6 * 14.0_f32.sqrt()) - Vec3::new(24.0, 5.0, 6.0))
            .length()
            < 1e-4
    );
}
