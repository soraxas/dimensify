#![cfg(feature = "inspect")]
use bevy::{
    camera::{RenderTargetInfo, Viewport},
    prelude::*,
    window::RequestRedraw,
};
use dimensify_camera::{
    CameraPose, InspectContext, InspectController, InspectEnvironment, InspectFrame,
    InspectGesture, InspectHit, InspectInput, InspectMotion, InspectPick, OrbitState,
    PoseTransition, TargetPose,
};
use std::time::Duration;

#[derive(Resource, Default)]
struct Input(InspectFrame);
#[derive(Resource)]
struct Scratch(Projection);

/// Simulated asynchronous picker: pending for `delay` polls, then answers.
#[derive(Resource, Default)]
struct Picker {
    delay: Option<u32>,
    hit: Option<Vec3>,
    polls: u32,
    cancels: u32,
}
impl InspectEnvironment for Picker {
    fn pick_hit(&mut self, _: &Camera, _: &GlobalTransform, _: Vec2) -> InspectPick {
        self.polls += 1;
        match (self.delay, self.hit) {
            (Some(delay), _) if self.polls <= delay => InspectPick::Pending,
            (None, _) if self.hit.is_some() => InspectPick::Pending,
            (_, Some(point)) => InspectPick::Hit(InspectHit {
                point,
                normal: Vec3::Z,
            }),
            _ => InspectPick::Miss,
        }
    }
    fn cancel_pick(&mut self) {
        self.cancels += 1;
    }
    fn constrain(&self, _: Vec3, proposed: Vec3) -> Vec3 {
        proposed
    }
}

#[allow(clippy::too_many_arguments)]
fn update(
    frame: Res<Input>,
    live: Res<CameraPose>,
    camera: Single<(&Camera, &Projection)>,
    mut state: ResMut<InspectController>,
    mut scratch: ResMut<Scratch>,
    mut orbit: ResMut<OrbitState>,
    mut target: ResMut<TargetPose>,
    mut transition: ResMut<PoseTransition>,
    mut redraw: MessageWriter<RequestRedraw>,
    mut picker: ResMut<Picker>,
) {
    let (camera, projection) = *camera;
    state.advance(
        &frame.0,
        InspectContext {
            camera,
            projection,
            scratch: scratch.reborrow().map_unchanged(|v| &mut v.0),
            live: &live,
            orbit: &mut orbit,
            target: &mut target,
            transition: &mut transition,
        },
        &mut redraw,
        &mut *picker,
    );
}

fn fixture() -> (App, Entity) {
    let pose = CameraPose::from_rotation(Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY, 10.0, 0.8);
    let mut orbit = OrbitState::default();
    orbit.publish_pose(&pose);
    let mut state = InspectController::default();
    state.editor.smoothing.orbit = Duration::ZERO;
    state.editor.smoothing.pan = Duration::ZERO;
    state.editor.smoothing.zoom = Duration::ZERO;
    let projection = Projection::Perspective(PerspectiveProjection {
        fov: 0.8,
        aspect_ratio: 1.0,
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
    app.insert_resource(pose)
        .insert_resource(TargetPose(pose))
        .insert_resource(orbit)
        .insert_resource(state)
        .insert_resource(Scratch(projection.clone()))
        .init_resource::<Input>()
        .init_resource::<Picker>()
        .init_resource::<PoseTransition>()
        .add_message::<RequestRedraw>()
        .add_systems(Update, update);
    let entity = app
        .world_mut()
        .spawn((camera, projection, pose.to_transform()))
        .id();
    (app, entity)
}

fn drive(app: &mut App, input: InspectInput) {
    app.world_mut().resource_mut::<Input>().0.input = input;
    app.update();
    let target = app.world().resource::<TargetPose>().0;
    *app.world_mut().resource_mut::<CameraPose>() = target;
}

#[test]
fn orbit_preserves_the_world_anchor_without_writing_the_render_transform() {
    let (mut app, entity) = fixture();
    let before = *app.world().resource::<CameraPose>();
    let input = InspectInput {
        gesture: Some(InspectGesture::Mouse(1, InspectMotion::Orbit)),
        pointer: Some(Vec2::new(650.0, 400.0)),
        ..default()
    };
    drive(&mut app, input);
    let anchor = app
        .world()
        .resource::<InspectController>()
        .editor
        .anchor_view_space()
        .unwrap()
        .as_vec3();
    let world = before.position + before.rotation() * anchor;
    drive(
        &mut app,
        InspectInput {
            delta: Vec2::new(60.0, 30.0),
            ..input
        },
    );
    let after = *app.world().resource::<CameraPose>();
    let anchor = app
        .world()
        .resource::<InspectController>()
        .editor
        .anchor_view_space()
        .unwrap()
        .as_vec3();
    assert!((after.position + after.rotation() * anchor).abs_diff_eq(world, 1e-4));
    assert!((after.rotation() * Vec3::X).y.abs() < 1e-5);
    assert!(!after.position.abs_diff_eq(before.position, 1e-3));
    assert_eq!(
        app.world().get::<Transform>(entity).unwrap().translation,
        before.position
    );
}

#[test]
fn pan_pinch_and_cancellation_share_one_target_and_never_replay_input() {
    let (mut app, _) = fixture();
    drive(
        &mut app,
        InspectInput {
            gesture: Some(InspectGesture::Touch(1, Some(2))),
            pointer: Some(Vec2::splat(500.0)),
            delta: Vec2::new(30.0, 10.0),
            zoom: 40.0,
            ..default()
        },
    );
    let moved = *app.world().resource::<CameraPose>();
    assert!(moved.position.x < 0.0 && moved.position.z < 10.0);
    drive(
        &mut app,
        InspectInput {
            cancel: true,
            ..default()
        },
    );
    drive(&mut app, InspectInput::default());
    assert_eq!(
        app.world().resource::<TargetPose>().0.position,
        moved.position
    );
    assert!(
        !app.world()
            .resource::<InspectController>()
            .editor
            .current_motion
            .is_moving()
    );
}

#[test]
fn a_view_request_is_consumed_once_even_when_input_is_suppressed() {
    let (mut app, _) = fixture();
    let desired = CameraPose::looking(Vec3::new(3.0, 4.0, 5.0), Vec3::ZERO, 1.2);
    app.world_mut()
        .resource_mut::<OrbitState>()
        .seed_from(&desired);
    app.world_mut().resource_mut::<Input>().0.motion_suppressed = true;
    app.update();
    assert!(
        app.world()
            .resource::<TargetPose>()
            .0
            .position
            .abs_diff_eq(desired.position, 1e-4)
    );
    assert_eq!(app.world().resource::<PoseTransition>().timer, 0.0);
    app.world_mut().resource_mut::<PoseTransition>().timer = 0.5;
    app.update();
    assert_eq!(app.world().resource::<PoseTransition>().timer, 0.5);
}

fn anchor_world(app: &App) -> Vec3 {
    let pose = *app.world().resource::<CameraPose>();
    let anchor = app
        .world()
        .resource::<InspectController>()
        .editor
        .anchor_view_space()
        .unwrap()
        .as_vec3();
    pose.position + pose.rotation() * anchor
}

#[test]
fn a_pending_pick_holds_the_gesture_then_pivots_on_the_hit_with_buffered_input() {
    let (mut app, _) = fixture();
    let surface = Vec3::new(1.0, 0.5, 2.0);
    *app.world_mut().resource_mut::<Picker>() = Picker {
        delay: Some(2),
        hit: Some(surface),
        ..default()
    };
    let before = *app.world().resource::<CameraPose>();
    let input = InspectInput {
        gesture: Some(InspectGesture::Mouse(1, InspectMotion::Orbit)),
        pointer: Some(Vec2::new(600.0, 450.0)),
        delta: Vec2::new(20.0, 0.0),
        ..default()
    };
    for _ in 0..2 {
        drive(&mut app, input);
        assert!(app.world().resource::<InspectController>().is_picking());
        assert_eq!(
            app.world().resource::<CameraPose>().position,
            before.position
        );
    }
    drive(&mut app, input);
    let state = app.world().resource::<InspectController>();
    assert!(!state.is_picking());
    assert_eq!(state.pivot().unwrap().0, surface);
    // All three frames of motion are applied once the pivot resolves.
    let moved = *app.world().resource::<CameraPose>();
    assert!(!moved.position.abs_diff_eq(before.position, 1e-3));
    assert!(anchor_world(&app).abs_diff_eq(surface, 1e-3));
    assert_eq!(app.world().resource::<Picker>().cancels, 1);
}

#[test]
fn an_unanswered_pick_expires_to_the_focus_plane() {
    let (mut app, _) = fixture();
    *app.world_mut().resource_mut::<Picker>() = Picker {
        hit: Some(Vec3::ZERO),
        ..default()
    };
    let input = InspectInput {
        gesture: Some(InspectGesture::Mouse(1, InspectMotion::Orbit)),
        pointer: Some(Vec2::splat(500.0)),
        ..default()
    };
    // Frame time, not wall time, drives expiry. Enough frames have passed but
    // only 80 ms of waiting (the starting frame's delta is not counted).
    app.world_mut().resource_mut::<Input>().0.delta_time = Duration::from_millis(10);
    for _ in 0..9 {
        drive(&mut app, input);
    }
    assert!(app.world().resource::<InspectController>().is_picking());
    // One long frame passes the 150 ms floor.
    app.world_mut().resource_mut::<Input>().0.delta_time = Duration::from_millis(100);
    drive(&mut app, input);
    let state = app.world().resource::<InspectController>();
    assert!(!state.is_picking());
    // Focus plane at the pose distance (10) straight ahead of the camera.
    assert!(state.pivot().unwrap().0.abs_diff_eq(Vec3::ZERO, 1e-3));
}

#[test]
fn an_unanswered_pick_expires_without_frame_time() {
    let (mut app, _) = fixture();
    *app.world_mut().resource_mut::<Picker>() = Picker {
        hit: Some(Vec3::ZERO),
        ..default()
    };
    let input = InspectInput {
        gesture: Some(InspectGesture::Mouse(1, InspectMotion::Orbit)),
        pointer: Some(Vec2::splat(500.0)),
        ..default()
    };
    // A zero (or paused) delta never reaches the time floor; more than 60 waiting
    // frames without elapsed time still end it (the starting frame is not counted).
    for _ in 0..61 {
        drive(&mut app, input);
    }
    assert!(app.world().resource::<InspectController>().is_picking());
    drive(&mut app, input);
    let state = app.world().resource::<InspectController>();
    assert!(!state.is_picking());
    assert!(state.pivot().unwrap().0.abs_diff_eq(Vec3::ZERO, 1e-3));
}

#[test]
fn a_fast_frame_rate_still_waits_for_the_time_floor() {
    let (mut app, _) = fixture();
    *app.world_mut().resource_mut::<Picker>() = Picker {
        hit: Some(Vec3::ZERO),
        ..default()
    };
    let input = InspectInput {
        gesture: Some(InspectGesture::Mouse(1, InspectMotion::Orbit)),
        pointer: Some(Vec2::splat(500.0)),
        ..default()
    };
    // 1000 fps.
    app.world_mut().resource_mut::<Input>().0.delta_time = Duration::from_millis(1);
    for _ in 0..100 {
        drive(&mut app, input);
    }
    // 99 ms of waiting over 100 frames: well past any frame count, under the floor.
    assert!(app.world().resource::<InspectController>().is_picking());
    app.world_mut().resource_mut::<Input>().0.delta_time = Duration::from_millis(60);
    drive(&mut app, input);
    assert!(!app.world().resource::<InspectController>().is_picking());
}
