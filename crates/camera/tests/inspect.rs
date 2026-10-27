#![cfg(feature = "inspect")]
use bevy::{
    camera::{RenderTargetInfo, Viewport},
    prelude::*,
    window::RequestRedraw,
};
use dimensify_camera::{
    CameraPose, InspectContext, InspectController, InspectFrame, InspectGesture, InspectInput,
    InspectMotion, OrbitState, PoseTransition, TargetPose,
};
use std::time::Duration;

#[derive(Resource, Default)]
struct Input(InspectFrame);
#[derive(Resource)]
struct Scratch(Projection);

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
        &(),
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
