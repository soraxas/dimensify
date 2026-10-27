//! Headless orbit, pan and zoom with no window, renderer or collision backend.
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

fn main() {
    let (mut app, entity) = fixture();
    let initial = app.world().get::<Transform>(entity).unwrap().translation;
    for (motion, delta, zoom) in [
        (InspectMotion::Orbit, Vec2::new(60.0, 30.0), 0.0),
        (InspectMotion::Pan, Vec2::new(30.0, 10.0), 0.0),
        (InspectMotion::Zoom, Vec2::ZERO, 40.0),
    ] {
        app.world_mut().resource_mut::<Input>().0.input = InspectInput {
            gesture: Some(InspectGesture::Wheel(motion)),
            pointer: Some(Vec2::splat(500.0)),
            delta,
            zoom,
            ..default()
        };
        app.update();
        let pose = app.world().resource::<TargetPose>().0;
        println!(
            "{motion:?}: eye={:?}, distance={}",
            pose.position, pose.distance
        );
        *app.world_mut().resource_mut::<CameraPose>() = pose;
    }
    app.world_mut().resource_mut::<Input>().0.input = InspectInput {
        cancel: true,
        ..default()
    };
    app.update();
    assert_eq!(
        app.world().get::<Transform>(entity).unwrap().translation,
        initial
    );
    println!("Targets computed headlessly; the application retains its render transform writer.");
}
