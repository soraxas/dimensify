//! Device-independent first-person input response. Consumers own input capture,
//! collision constraints and the final camera write.
//!
//! Adapted from SuperSplat Viewer b797cc4, MIT. Copyright (c) 2011-2026
//! PlayCanvas Ltd. See docs/legal/playcanvas-MIT.txt in the distribution.

use crate::shortest_angle;
use bevy::prelude::*;

/// Accumulate yaw/pitch input and drain its smoothing tail along the short arc.
/// Angles and deltas are radians. Clearing `target` cancels queued look motion.
pub fn smooth_look(current: Vec2, target: &mut Option<Vec2>, delta: Vec2, dt: f32) -> Vec2 {
    let mut next = target.unwrap_or(current) + delta;
    next.x = next.x.rem_euclid(std::f32::consts::TAU);
    next.y = next
        .y
        .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
    let k = 1.0 - 0.95_f32.powf(dt.max(0.0) * 1000.0);
    let angles = Vec2::new(
        (current.x + shortest_angle(next.x - current.x) * k).rem_euclid(std::f32::consts::TAU),
        current.y + (next.y - current.y) * k,
    );
    *target = Some(next);
    angles
}

/// Semantic Fly input after device gains, UI capture and navigation arbitration.
#[derive(Clone, Copy, Debug)]
pub struct FlyInput {
    /// Keyboard/joystick direction, normalized before applying speed.
    pub move_local: Vec3,
    /// Unsmoothed displacement in the displayed right/up/forward basis, in metres.
    pub pointer_move: Vec3,
    /// Unsmoothed world-space navigation displacement, in metres.
    pub world_move: Vec3,
    /// Additional unsmoothed displacement along the displayed forward direction.
    pub nav_forward: f32,
    /// Keyboard speed in metres per second; pointer impulses are independent.
    pub speed: f32,
    /// Follow full view orientation; false keeps keyboard movement level.
    pub along_view: bool,
}

impl Default for FlyInput {
    fn default() -> Self {
        Self {
            move_local: Vec3::ZERO,
            pointer_move: Vec3::ZERO,
            world_move: Vec3::ZERO,
            nav_forward: 0.0,
            speed: 4.0,
            along_view: true,
        }
    }
}

/// Integrate keyboard momentum and return world velocity for optional collision.
/// Nonzero `local_velocity` requires another frame even if collision blocks the
/// eye. Clearing it cancels momentum; pointer impulses never enter that state.
pub fn fly_velocity(
    local_velocity: &mut Vec3,
    rotation: Quat,
    yaw: f32,
    input: &FlyInput,
    dt: f32,
) -> Vec3 {
    let forward = rotation * Vec3::NEG_Z;
    let right = rotation * Vec3::X;
    let view_up = rotation * Vec3::Y;
    let (fwd, up) = if input.along_view {
        (forward, view_up)
    } else {
        (Vec3::new(-yaw.sin(), 0.0, -yaw.cos()), Vec3::Y)
    };
    let desired_velocity = input.move_local.normalize_or_zero() * input.speed;
    let damping: f32 = if desired_velocity.length_squared() > local_velocity.length_squared() {
        0.992
    } else {
        0.993
    };
    let k = 1.0 - damping.powf(dt.max(0.0) * 1000.0);
    *local_velocity = local_velocity.lerp(desired_velocity, k);
    if desired_velocity == Vec3::ZERO && local_velocity.length_squared() < 1e-4 {
        *local_velocity = Vec3::ZERO;
    }
    let manual = right * local_velocity.x + up * local_velocity.y + fwd * local_velocity.z;
    let pointer = right * input.pointer_move.x
        + view_up * input.pointer_move.y
        + forward * input.pointer_move.z;
    let displacement_velocity = if dt > 0.0 {
        (input.world_move + forward * input.nav_forward + pointer) / dt
    } else {
        Vec3::ZERO
    };
    manual + displacement_velocity
}
