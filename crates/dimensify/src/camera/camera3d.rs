// NOTE: this is mostly taken from the `iMplode-nZ/bevy-orbit-controls` projects but
//       with some modifications like Panning, and 2D support.
//       Most of these modifications have been contributed upstream.

use bevy::input::mouse::MouseMotion;
use bevy::input::mouse::MouseScrollUnit::{Line, Pixel};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::render::camera::Camera;
use bevy_egui::{EguiContexts, EguiWantsInput, egui};
use std::ops::RangeInclusive;

const LINE_TO_PIXEL_RATIO: f32 = 0.1;

#[derive(Component)]
pub struct OrbitCamera {
    pub x: f32,
    pub y: f32,
    pub pitch_range: RangeInclusive<f32>,
    pub distance: f32,
    pub center: Vec3,
    pub rotate_sensitivity: f32,
    pub pan_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub rotate_button: MouseButton,
    pub pan_button: MouseButton,
    pub enabled: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        OrbitCamera {
            x: 0.0,
            y: std::f32::constants::FRAC_PI_2,
            pitch_range: 0.01..=3.13,
            distance: 5.0,
            center: Vec3::ZERO,
            rotate_sensitivity: 1.0,
            pan_sensitivity: 1.0,
            zoom_sensitivity: 0.8,
            rotate_button: MouseButton::Left,
            pan_button: MouseButton::Right,
            enabled: true,
        }
    }
}

pub struct OrbitCameraPlugin;
impl OrbitCameraPlugin {
    #[allow(clippy::type_complexity)]
    fn update_transform_system(
        mut query: Query<(&OrbitCamera, &mut Transform), (Changed<OrbitCamera>, With<Camera>)>,
    ) {
        for (camera, mut transform) in query.iter_mut() {
            let rot = Quat::from_axis_angle(Vec3::Y, camera.x)
                * Quat::from_axis_angle(-Vec3::X, camera.y);
            transform.translation = (rot * Vec3::Y) * camera.distance + camera.center;
            transform.look_at(camera.center, Vec3::Y);
        }
    }

    fn mouse_motion_system(
        time: Res<Time>,
        mut mouse_motion_events: EventReader<MouseMotion>,
        mouse_button_input: Res<ButtonInput<MouseButton>>,
        mut egui_contexts: EguiContexts,
        mut query: Query<(&mut OrbitCamera, &mut Transform, &mut Camera)>,
    ) {
        let mut delta = Vec2::ZERO;
        for event in mouse_motion_events.read() {
            delta += event.delta;
        }
        let ctx = egui_contexts.ctx_mut();
        let egui_delta = ctx.input(|input| input.pointer.delta());
        let egui_pointer_pos = ctx.pointer_latest_pos();
        let pixels_per_point = ctx.pixels_per_point();

        for (mut camera, transform, _) in query.iter_mut() {
            if !camera.enabled {
                continue;
            }

            let cursor_in_viewport = match (egui_pointer_pos, camera.viewport.as_ref()) {
                (Some(pos), Some(viewport)) => {
                    let min = viewport.physical_position.as_vec2() / pixels_per_point;
                    let max = min + (viewport.physical_size.as_vec2() / pixels_per_point);
                    pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y
                }
                _ => false,
            };

            let rotate_pressed = if cursor_in_viewport {
                pointer_button_down(ctx, camera.rotate_button)
                    .unwrap_or_else(|| mouse_button_input.pressed(camera.rotate_button))
            } else {
                mouse_button_input.pressed(camera.rotate_button)
            };

            let pan_pressed = if cursor_in_viewport {
                pointer_button_down(ctx, camera.pan_button)
                    .unwrap_or_else(|| mouse_button_input.pressed(camera.pan_button))
            } else {
                mouse_button_input.pressed(camera.pan_button)
            };

            let motion = if cursor_in_viewport { egui_delta } else { delta };

            if rotate_pressed {
                camera.x -= motion.x * camera.rotate_sensitivity * time.delta_seconds();
                camera.y -= motion.y * camera.rotate_sensitivity * time.delta_seconds();
                camera.y = camera
                    .y
                    .max(*camera.pitch_range.start())
                    .min(*camera.pitch_range.end());
            }

            if pan_pressed {
                let right_dir = transform.rotation * -Vec3::X;
                let up_dir = transform.rotation * Vec3::Y;
                let pan_vector = (motion.x * right_dir + motion.y * up_dir)
                    * camera.pan_sensitivity
                    * time.delta_seconds();
                camera.center += pan_vector;
            }
        }
    }

    fn zoom_system(
        mut mouse_wheel_events: EventReader<MouseWheel>,
        mut egui_contexts: EguiContexts,
        mut query: Query<(&mut OrbitCamera, &Camera)>,
    ) {
        let mut total = 0.0;
        for event in mouse_wheel_events.read() {
            total += event.y
                * match event.unit {
                    Line => 1.0,
                    Pixel => LINE_TO_PIXEL_RATIO,
                };
        }
        for (mut camera, _) in query.iter_mut() {
            if camera.enabled && total.abs() > f32::EPSILON {
                camera.distance *= camera.zoom_sensitivity.powf(total);
            }
        }

        let ctx = egui_contexts.ctx_mut();
        if total.abs() > f32::EPSILON {
            return;
        }
        let egui_scroll = ctx.input(|input| input.raw_scroll_delta.y);
        let egui_pointer_pos = ctx.pointer_latest_pos();
        let pixels_per_point = ctx.pixels_per_point();
        for (mut camera, camera_view) in query.iter_mut() {
            if !camera.enabled {
                continue;
            }
            let cursor_in_viewport = match (egui_pointer_pos, camera_view.viewport.as_ref()) {
                (Some(pos), Some(viewport)) => {
                    let min = viewport.physical_position.as_vec2() / pixels_per_point;
                    let max = min + (viewport.physical_size.as_vec2() / pixels_per_point);
                    pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y
                }
                _ => false,
            };
            if cursor_in_viewport && egui_scroll.abs() > f32::EPSILON {
                camera.distance *= camera.zoom_sensitivity.powf(egui_scroll * 0.01);
            }
        }
    }
}
impl Plugin for OrbitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::mouse_motion_system)
            .add_systems(Update, Self::zoom_system)
            .add_systems(Update, Self::update_transform_system)
            .add_systems(Update, egui_focus);
    }
}

fn egui_focus(
    mut ui_context: EguiContexts,
    egui_wants_input: Option<Res<EguiWantsInput>>,
    mut cameras: Query<(&Camera, &mut OrbitCamera)>,
) {
    let ctx = ui_context.ctx_mut();
    let wants_pointer = ctx.wants_pointer_input();
    let is_using_pointer = egui_wants_input
        .as_ref()
        .map_or(false, |wants| wants.is_using_pointer());

    let cursor_pos = ctx.pointer_latest_pos();
    let pixels_per_point = ctx.pixels_per_point();

    for (camera, mut orbit) in cameras.iter_mut() {
        let cursor_in_viewport = match (cursor_pos, camera.viewport.as_ref()) {
            (Some(pos), Some(viewport)) => {
                let min = viewport.physical_position.as_vec2() / pixels_per_point;
                let max = min + (viewport.physical_size.as_vec2() / pixels_per_point);
                pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y
            }
            _ => false,
        };

        let camera_enabled = if cursor_in_viewport {
            // Always allow orbit controls inside the viewport; egui should not block navigation there.
            true
        } else {
            // Outside the viewport, respect egui pointer intent.
            !(wants_pointer || is_using_pointer)
        };
        orbit.enabled = camera_enabled;
    }
}

fn pointer_button_down(ctx: &egui::Context, button: MouseButton) -> Option<bool> {
    let egui_button = match button {
        MouseButton::Left => egui::PointerButton::Primary,
        MouseButton::Right => egui::PointerButton::Secondary,
        MouseButton::Middle => egui::PointerButton::Middle,
        _ => return None,
    };
    Some(ctx.input(|input| input.pointer.button_down(egui_button)))
}
