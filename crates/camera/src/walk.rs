//! Fixed-step Walk response adapted from SuperSplat Viewer e83a447e.
//! MIT, Copyright (c) 2011-2026 PlayCanvas Ltd.; docs/legal/playcanvas-MIT.txt.
//! Geometry queries and collision resolution are supplied by the consumer.

use bevy::prelude::*;
use std::time::Duration;

const FIXED_DT: f64 = 1.0 / 60.0;
const MAX_SUBSTEPS: usize = 10;
/// Maximum elapsed time consumed by one Walk update; excess time is discarded.
pub const WALK_MAX_FRAME_DELTA: f64 = MAX_SUBSTEPS as f64 * FIXED_DT;
const GRAVITY: f32 = 9.8;
const JUMP_SPEED: f32 = 4.0;
/// Suspension clearance above terrain, in metres.
pub const WALK_HOVER_HEIGHT: f32 = 0.2;

/// Eye and vertical state carried between Walk updates.
#[derive(Clone, Copy, Debug)]
pub struct WalkBody {
    /// Display-interpolated eye position in Bevy Y-up coordinates.
    pub position: Vec3,
    /// Vertical velocity in metres per second.
    pub vertical_velocity: f32,
    /// Whether suspension or collision currently supports the walker.
    pub grounded: bool,
    /// Eye offset above the capsule bottom, excluding suspension clearance.
    pub eye_height: f32,
}

/// Semantic movement after device scaling and input ownership arbitration.
#[derive(Clone, Copy, Debug)]
pub struct WalkInput {
    /// Local keyboard/joystick direction; only X/Z are used.
    pub move_local: Vec3,
    /// Pointer displacement in local metres; Y is not a jump signal.
    pub pointer_move: Vec3,
    /// Navigation displacement in world metres; Y is ignored.
    pub world_move: Vec3,
    /// Additional displacement along planar forward, in metres.
    pub nav_forward: f32,
    /// Keyboard speed in metres per second.
    pub speed: f32,
    /// Jump edge retained until the next available fixed step.
    pub jump: bool,
    /// Heading around world +Y, in radians.
    pub yaw: f32,
}

impl Default for WalkInput {
    fn default() -> Self {
        Self {
            move_local: Vec3::ZERO,
            pointer_move: Vec3::ZERO,
            world_move: Vec3::ZERO,
            nav_forward: 0.0,
            speed: 4.0,
            jump: false,
            yaw: 0.0,
        }
    }
}

/// Terrain attachment is independent of queued input and horizontal momentum.
#[derive(Clone, Copy, Debug)]
pub enum WalkSupport {
    /// Temporary terrain level while scene collision is unavailable or embedded.
    Held(f32),
    /// The environment supplies terrain and collision queries.
    Collision,
}

/// Result of resolving a fixed-step eye movement against consumer geometry.
pub struct WalkMove {
    /// Resolved eye position.
    pub position: Vec3,
    /// Vertical velocity after contact projection.
    pub vertical_velocity: f32,
    /// Ground contact, including existing suspension support.
    pub grounded: bool,
}

/// Optional terrain/collision boundary; no physics engine is required.
pub trait WalkEnvironment {
    /// Whether the walker can attach to collision at this eye position.
    fn is_clear(&mut self, eye: Vec3, eye_height: f32) -> bool;
    /// Terrain level beneath the eye, or no supporting ground.
    fn terrain_height(&mut self, eye: Vec3, eye_height: f32) -> Option<f32>;
    /// Constrain one fixed-step movement and return its vertical contact state.
    fn move_eye(
        &mut self,
        eye: Vec3,
        velocity: Vec3,
        eye_height: f32,
        grounded: bool,
        dt: Duration,
    ) -> WalkMove;
}

/// Fixed-step Walk state, including queued impulses and display interpolation.
pub struct WalkMotion {
    position: Vec3,
    previous_position: Vec3,
    /// Horizontal world-space velocity (X/Z), retained while blocked.
    pub velocity: Vec2,
    pending_move: Vec3,
    pending_world_move: Vec3,
    pending_jump: bool,
    jumping: bool,
    accumulator: f64,
}

impl WalkMotion {
    /// Seed from the visible body; preserve ascent when input momentum is reset.
    pub fn new(body: &WalkBody) -> Self {
        Self {
            position: body.position,
            previous_position: body.position,
            velocity: Vec2::ZERO,
            pending_move: Vec3::ZERO,
            pending_world_move: Vec3::ZERO,
            pending_jump: false,
            jumping: !body.grounded && body.vertical_velocity > 0.0,
            accumulator: 0.0,
        }
    }

    /// Hold the current simulation level until scene collision can attach.
    pub fn hold_level(&self, eye_height: f32) -> WalkSupport {
        WalkSupport::Held(self.position.y - eye_height - WALK_HOVER_HEIGHT)
    }

    /// Integrate at 60 Hz and interpolate the display eye; return pending frame demand.
    /// Displacements and jump edges survive frames without a fixed step. Catch-up
    /// is bounded to ten steps, discarding excess elapsed time.
    pub fn advance(
        &mut self,
        body: &mut WalkBody,
        support: &mut WalkSupport,
        input: &WalkInput,
        dt: f64,
        environment: &mut impl WalkEnvironment,
    ) -> bool {
        let dt = dt.min(WALK_MAX_FRAME_DELTA);
        let eye_height = body.eye_height;
        let forward = Vec3::new(-input.yaw.sin(), 0.0, -input.yaw.cos());
        let right = Vec3::new(input.yaw.cos(), 0.0, -input.yaw.sin());
        let local = input.move_local.xz().normalize_or_zero() * input.speed * dt as f32;
        // Preserve pointer impulses across display frames without a physics step.
        // Walk stays on the ground plane: vertical pan is not a jump signal.
        self.pending_move += Vec3::new(
            local.x + input.pointer_move.x,
            0.0,
            local.y + input.nav_forward + input.pointer_move.z,
        );
        self.pending_world_move += Vec3::new(input.world_move.x, 0.0, input.world_move.z);
        self.pending_jump |= input.jump;
        self.accumulator = (self.accumulator + dt).min(WALK_MAX_FRAME_DELTA);
        // Time uses integer nanoseconds. Tolerate that quantization at exact 60/120 Hz
        // boundaries rather than occasionally delaying a whole physics step.
        let steps = ((self.accumulator + 1e-7) / FIXED_DT).floor() as usize;
        if steps > 0 {
            let movement = (right * self.pending_move.x
                + forward * self.pending_move.z
                + self.pending_world_move)
                / steps as f32;
            let step_dt = FIXED_DT as f32;
            for _ in 0..steps {
                self.previous_position = self.position;
                if matches!(support, WalkSupport::Held(_))
                    && environment.is_clear(self.position, eye_height)
                {
                    *support = WalkSupport::Collision;
                }
                if self.pending_jump && body.grounded {
                    body.vertical_velocity = JUMP_SPEED;
                    body.grounded = false;
                    self.pending_jump = false;
                    self.jumping = true;
                }
                let terrain_y = match support {
                    WalkSupport::Held(y) => Some(*y),
                    WalkSupport::Collision => environment.terrain_height(self.position, eye_height),
                };
                terrain_vertical_step(self, body, terrain_y, eye_height, step_dt);
                let acceleration = if body.grounded { 7.0 } else { 1.0 };
                let damping = if body.grounded { 0.99_f32 } else { 0.998 };
                self.velocity += movement.xz() * acceleration;
                self.velocity *= damping.powf(step_dt * 1000.0);
                let velocity = Vec3::new(self.velocity.x, body.vertical_velocity, self.velocity.y);
                let (next_eye, grounded) = if matches!(support, WalkSupport::Collision) {
                    let resolved = environment.move_eye(
                        self.position,
                        velocity,
                        eye_height,
                        body.grounded,
                        Duration::from_secs_f64(FIXED_DT),
                    );
                    body.vertical_velocity = resolved.vertical_velocity;
                    (resolved.position, resolved.grounded)
                } else {
                    (self.position + velocity * step_dt, body.grounded)
                };
                body.grounded = grounded;
                self.position = next_eye;
                self.accumulator = (self.accumulator - FIXED_DT).max(0.0);
            }
            self.pending_move = Vec3::ZERO;
            self.pending_world_move = Vec3::ZERO;
            self.pending_jump = false;
        }
        let alpha = (self.accumulator / FIXED_DT).clamp(0.0, 1.0) as f32;
        body.position = self.previous_position.lerp(self.position, alpha);
        // Wall contact can hide this remaining controller work from pose-based
        // redraw detection. Bound the tail below 1 mm/s so idle demand expires.
        let pending = self.pending_move != Vec3::ZERO
            || self.pending_world_move != Vec3::ZERO
            || self.pending_jump;
        if !pending && self.velocity.length_squared() < 1e-6 {
            self.velocity = Vec2::ZERO;
        }
        pending || self.velocity != Vec2::ZERO
    }
}

fn terrain_vertical_step(
    motion: &mut WalkMotion,
    body: &mut WalkBody,
    terrain_y: Option<f32>,
    eye_height: f32,
    dt: f32,
) {
    if body.vertical_velocity < 0.0 {
        motion.jumping = false;
    }
    if let Some(terrain_y) = terrain_y.filter(|_| !motion.jumping) {
        let target_y = terrain_y + WALK_HOVER_HEIGHT + eye_height;
        let displacement = motion.position.y - target_y;
        if displacement > 0.1 {
            body.vertical_velocity -= GRAVITY * dt;
            if motion.position.y + body.vertical_velocity * dt <= target_y {
                motion.position.y = target_y;
                body.vertical_velocity = 0.0;
            }
            body.grounded = false;
        } else {
            body.vertical_velocity += (-800.0 * displacement - 57.0 * body.vertical_velocity) * dt;
            body.grounded = true;
        }
    } else {
        body.vertical_velocity -= GRAVITY * dt;
        body.grounded = false;
    }
    // Preserve the existing landing-frame jump contract after acquiring terrain.
    if motion.pending_jump && body.grounded && !motion.jumping {
        body.vertical_velocity = JUMP_SPEED;
        body.grounded = false;
        motion.jumping = true;
        motion.pending_jump = false;
    }
}
