//! Optional desktop/touch navigation adapter with one camera transform writer.
use bevy::ecs::schedule::SystemSet;

/// Camera controlled by the optional viewer navigation plugin.
#[derive(bevy::prelude::Component, Default)]
pub struct ViewerCamera;

/// Consumer-owned bounds, input capture and optional presentation override.
#[derive(bevy::prelude::Resource)]
pub struct NavigationConfig {
    pub bounds: Option<(bevy::prelude::Vec3, bevy::prelude::Vec3)>,
    pub blocked: bool,
    pub home: crate::CameraPose,
    pub override_pose: Option<crate::CameraPose>,
}
impl Default for NavigationConfig {
    fn default() -> Self {
        Self {
            bounds: None,
            blocked: false,
            home: home_pose(),
            override_pose: None,
        }
    }
}

/// Order consumer input policy before this set and presentation publication after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NavigationSystems;

/// Install optional device polling; camera-only/headless consumers leave this disabled.
pub struct ViewerNavigationPlugin;
impl bevy::prelude::Plugin for ViewerNavigationPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.init_resource::<NavigationMode>()
            .init_resource::<NavigationConfig>()
            .init_resource::<NavigationProjection>()
            .init_resource::<crate::OrbitState>()
            .add_message::<CameraAction>()
            .add_systems(bevy::prelude::Update, navigate.in_set(NavigationSystems));
    }
}

use crate::{
    CameraPose, FlyInput, InspectContext, InspectController, InspectDrag, InspectFrame,
    InspectGesture, OrbitState, PoseTransition, TargetPose, TouchGestureFrame, TouchMotionTracker,
    TouchSample, fly_velocity, inspect_wheel_input, perspective_fit_distance, pointer_drag_owned,
    smooth_look,
};
use bevy::{
    input::{
        mouse::{MouseMotion, MouseScrollUnit, MouseWheel},
        touch::Touches,
    },
    prelude::*,
    window::{PrimaryWindow, RequestRedraw},
};

/// Local camera interaction mode; switching preserves the visible pose.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationMode {
    #[default]
    Fly,
    Orbit,
}

/// Semantic camera requests emitted by controls or consuming applications.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraAction {
    Reset,
    ToggleMode,
    Frame,
}

/// Origin pose looking along the renderer's negative Z axis.
pub fn home_pose() -> CameraPose {
    CameraPose::from_rotation(Vec3::ZERO, Quat::IDENTITY, 1., std::f32::consts::FRAC_PI_4)
}

#[derive(Resource)]
pub(super) struct NavigationProjection(Projection);
impl Default for NavigationProjection {
    fn default() -> Self {
        Self(Projection::Perspective(default()))
    }
}

#[derive(Default)]
pub(super) struct Navigation {
    camera: Option<Entity>,
    pose: CameraPose,
    target: TargetPose,
    transition: PoseTransition,
    inspect: InspectController,
    drag: InspectDrag,
    left: bool,
    right: bool,
    velocity: Vec3,
    look_target: Option<Vec2>,
    touch: TouchMotionTracker,
    touch_active: bool,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
/// Adapt local devices and framing to shared inspect motion; write the final pose once.
pub(super) fn navigate(
    time: Res<Time>,
    config: Res<NavigationConfig>,
    mut state: Local<Navigation>,
    controls: (
        ResMut<NavigationMode>,
        MessageReader<CameraAction>,
        Option<Res<Touches>>,
        Option<Res<bevy::picking::hover::HoverMap>>,
    ),
    mut scratch: ResMut<NavigationProjection>,
    mut orbit: ResMut<OrbitState>,
    mut cameras: Query<(Entity, &Camera, &mut Transform, &mut Projection), With<ViewerCamera>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    interactions: (Query<&bevy::picking::hover::Hovered>, Query<(), With<Node>>),
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let (mut mode, mut actions, touches, hover_map) = controls;
    let editing_url = config.blocked;
    let Ok((entity, camera, mut transform, mut projection)) = cameras.single_mut() else {
        return;
    };
    if state.camera != Some(entity) {
        let fov = match &*projection {
            Projection::Perspective(p) => p.fov,
            _ => home_pose().fov,
        };
        let pose = CameraPose::from_rotation(
            transform.translation,
            transform.rotation,
            orbit.distance,
            fov,
        );
        orbit.seed_from(&pose);
        *state = Navigation {
            camera: Some(entity),
            pose,
            target: TargetPose(pose),
            ..default()
        };
    }
    let mut frame_requested = false;
    let mut changed = false;
    for action in actions.read() {
        match action {
            CameraAction::Frame => {
                frame_requested = true;
                continue;
            }
            CameraAction::Reset => state.pose = config.home,
            CameraAction::ToggleMode => {
                *mode = if *mode == NavigationMode::Fly {
                    NavigationMode::Orbit
                } else {
                    NavigationMode::Fly
                }
            }
        }
        changed = true;
    }
    if changed {
        let pose = state.pose;
        orbit.seed_from(&pose);
        state.target = TargetPose(pose);
        state.transition = PoseTransition::default();
        state.inspect.cancel();
        state.velocity = Vec3::ZERO;
        state.look_target = None;
        state.left = false;
        state.right = false;
    }
    if let Some(pose) = config.override_pose {
        *state = Navigation {
            camera: Some(entity),
            pose,
            target: TargetPose(pose),
            ..default()
        };
        orbit.seed_from(&pose);
        *transform = pose.to_transform();
        if let Projection::Perspective(p) = &mut *projection {
            p.fov = pose.fov;
        }
        return;
    }
    let focused = window.as_ref().is_some_and(|window| window.focused);
    let requested =
        frame_requested || (focused && !editing_url && keys.just_pressed(KeyCode::KeyF));
    let bounds = requested.then_some(config.bounds).flatten();
    let framing = bounds.is_some();
    if let Some((min, max)) = bounds {
        orbit.target_focus = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.5);
        let (fov, viewport) = match &*projection {
            Projection::Perspective(p) => (p.fov, Vec2::new(p.aspect_ratio, 1.)),
            _ => (std::f32::consts::FRAC_PI_3, Vec2::ONE),
        };
        // Preserve the file viewer's existing 20% framing margin.
        orbit.target_distance =
            perspective_fit_distance(radius * (1.2 / 1.05), fov, viewport).clamp(0.01, 1e7);
        orbit.request_view();
        state.pose = orbit.requested_pose();
        state.target.0 = state.pose;
        state.inspect.cancel();
        state.velocity = Vec3::ZERO;
        state.look_target = None;
        state.transition = PoseTransition::default();
        state.left = false;
        state.right = false;
    }
    // Read each event once even when UI or focus suppresses it.
    let scale = window.as_ref().map_or(1.0, |w| w.scale_factor());
    let delta: Vec2 = motion.read().map(|m| m.delta / scale).sum();
    let mut scroll = 0.0;
    let mut wheel_pan = Vec2::ZERO;
    let mut wheel_orbit = Vec2::ZERO;
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for event in wheel.read() {
        // Winit signs oppose logical pointer/DOM wheel offsets.
        let pixels = -Vec2::new(event.x, event.y)
            * if event.unit == MouseScrollUnit::Pixel {
                1.0
            } else {
                50.0
            };
        if control {
            wheel_orbit += pixels;
        } else if shift {
            wheel_pan += pixels;
        } else {
            scroll += event.y
                * if event.unit == MouseScrollUnit::Pixel {
                    0.01
                } else {
                    1.0
                };
        }
    }
    let over_ui = interactions.0.iter().any(|hovered| hovered.get());
    let pointer = window.as_ref().and_then(|w| w.cursor_position());
    let in_viewport = pointer.is_some_and(|point| {
        camera
            .logical_viewport_rect()
            .is_some_and(|r| r.contains(point))
    });
    let allowed = focused && !framing && !changed && !editing_url;
    let samples: Vec<_> = touches
        .as_ref()
        .map(|touches| {
            touches
                .iter()
                .map(|touch| TouchSample {
                    id: touch.id(),
                    position: touch.position(),
                    start_position: touch.start_position(),
                    just_started: touches.just_pressed(touch.id()),
                })
                .collect()
        })
        .unwrap_or_default();
    let canceled = touches
        .as_ref()
        .is_some_and(|touches| touches.iter_just_canceled().next().is_some());
    let touching = !samples.is_empty() || state.touch_active || canceled;
    // A stationary mouse can still hover the toolbar while a finger is in the
    // scene. Each touch must use its own picking ownership, not combined hover.
    let touch_over_ui = hover_map.as_ref().map_or(over_ui, |hover_map| {
        samples.iter().any(|sample| {
            hover_map
                .get(&bevy::picking::pointer::PointerId::Touch(sample.id))
                .is_some_and(|hits| hits.keys().any(|entity| interactions.1.contains(*entity)))
        })
    });
    state.touch_active = !samples.is_empty();
    state.touch.update_owned(
        &samples,
        allowed && !touch_over_ui && !canceled,
        camera.logical_viewport_rect(),
    );
    let (first, second) = state.touch.current();
    let touch_motion = state.touch.gesture();
    let touch_blocked = canceled || (!samples.is_empty() && first.is_none());
    let start_allowed = allowed && in_viewport && !over_ui && !touching;
    state.left = allowed
        && !touching
        && pointer_drag_owned(
            state.left,
            mouse.pressed(MouseButton::Left),
            mouse.just_pressed(MouseButton::Left),
            mouse.just_released(MouseButton::Left),
            start_allowed,
        );
    state.right = allowed
        && !touching
        && pointer_drag_owned(
            state.right,
            mouse.pressed(MouseButton::Right),
            mouse.just_pressed(MouseButton::Right),
            mouse.just_released(MouseButton::Right),
            start_allowed,
        );
    let left = state.left;
    let right = state.right;
    let gesture = state.drag.update_owned(left, right, shift);
    let mut input = if start_allowed {
        inspect_wheel_input(pointer, wheel_pan, wheel_orbit, scroll, false)
    } else {
        default()
    };
    if gesture.is_some() {
        input.gesture = gesture;
        input.pointer = pointer;
        input.delta = delta;
    }
    if let Some(first) = first {
        input.gesture = Some(InspectGesture::Touch(
            first.id,
            second.map(|sample| sample.id),
        ));
        input.pointer = Some(second.map_or(first.position, |second| {
            first.position.midpoint(second.position)
        }));
        input.delta = match touch_motion {
            TouchGestureFrame::OneFinger { motion }
            | TouchGestureFrame::TwoFinger { motion, .. } => motion,
            TouchGestureFrame::None => Vec2::ZERO,
        };
        if let TouchGestureFrame::TwoFinger { pinch, .. } = touch_motion {
            input.zoom = pinch * (0.4 / 60.0) / 0.05 * 50.0;
        }
    }
    input.cancel |= touch_blocked;
    if *mode == NavigationMode::Fly {
        let enabled =
            allowed && !(if touching { touch_over_ui } else { over_ui }) && !touch_blocked;
        if !enabled {
            state.velocity = Vec3::ZERO;
            state.look_target = None;
        } else {
            let current = Vec2::new(state.pose.yaw, state.pose.pitch);
            let mut look_delta = if left {
                -delta * 0.216_f32.to_radians()
            } else {
                Vec2::ZERO
            };
            if let TouchGestureFrame::OneFinger { motion } = touch_motion {
                let aspect = match &*projection {
                    Projection::Perspective(p) => p.aspect_ratio,
                    _ => 1.0,
                };
                let longer_fov = 2.0 * ((state.pose.fov * 0.5).tan() * aspect.max(1.0)).atan();
                look_delta = motion * (0.18 * longer_fov.to_degrees() / 120.0).to_radians();
            }
            let (touch_pan, touch_forward) = match touch_motion {
                TouchGestureFrame::TwoFinger { motion, pinch } => (
                    Vec3::new(-motion.x, motion.y, 0.) * 0.005,
                    pinch * (0.4 / 60.0),
                ),
                _ => (Vec3::ZERO, 0.0),
            };
            let angles = smooth_look(
                current,
                &mut state.look_target,
                look_delta,
                time.delta_secs(),
            );
            let rotation = Quat::from_rotation_y(angles.x) * Quat::from_rotation_x(angles.y);
            let direction = |positive, negative| {
                f32::from(keys.pressed(positive)) - f32::from(keys.pressed(negative))
            };
            let fly = FlyInput {
                move_local: Vec3::new(
                    direction(KeyCode::KeyD, KeyCode::KeyA),
                    direction(KeyCode::KeyE, KeyCode::KeyQ),
                    direction(KeyCode::KeyW, KeyCode::KeyS),
                ),
                pointer_move: touch_pan
                    + if right {
                        Vec3::new(-delta.x, delta.y, 0.) * 0.005
                    } else {
                        Vec3::ZERO
                    },
                nav_forward: touch_forward + if start_allowed { scroll * 0.05 } else { 0. },
                speed: 4.
                    * if control {
                        0.25
                    } else if shift {
                        4.
                    } else {
                        1.
                    },
                ..default()
            };
            let velocity = fly_velocity(
                &mut state.velocity,
                rotation,
                angles.x,
                &fly,
                time.delta_secs(),
            );
            let position = state.pose.position + velocity * time.delta_secs();
            state.pose =
                CameraPose::from_rotation(position, rotation, state.pose.distance, state.pose.fov);
            state.target.0 = state.pose;
            orbit.seed_from(&state.pose);
        }
    } else {
        let Navigation {
            pose,
            target,
            transition,
            inspect,
            ..
        } = &mut *state;
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
                projection: &projection,
                scratch: scratch.reborrow().map_unchanged(|p| &mut p.0),
                live: pose,
                orbit: &mut orbit,
                target,
                transition,
            },
            &mut redraw,
            &(),
        );
        *pose = transition.advance(&target.0, time.delta_secs());
    }
    let pose = &state.pose;
    let final_transform = pose.to_transform();
    if *transform != final_transform {
        *transform = final_transform;
    }
    match &mut *projection {
        Projection::Perspective(p) => {
            p.fov = pose.fov;
            p.near = (pose.distance * 0.0001).max(0.0001);
            p.far = (pose.distance * 100.).max(1000.);
        }
        Projection::Orthographic(p) => p.scale = orbit.ortho_scale,
        _ => {}
    }
}

#[cfg(test)]
mod tests;
