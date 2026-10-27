use crate::CameraPose;
use bevy::prelude::*;

/// The active controller's requested pose, before the single writer advances it.
#[derive(Resource, Default)]
pub struct TargetPose(pub CameraPose);

/// Direct eye/rotation transition between modes or semantic view requests.
/// This state computes poses; it never writes a camera transform itself.
#[derive(Resource)]
pub struct PoseTransition {
    /// Visible pose captured when the transition starts.
    pub from: CameraPose,
    /// Normalized progress; one means the controller target is used directly.
    pub timer: f32,
    /// Reciprocal duration in seconds.
    pub speed: f32,
}

impl Default for PoseTransition {
    fn default() -> Self {
        Self {
            from: CameraPose::default(),
            timer: 1.0,
            speed: 1.0,
        }
    }
}

impl PoseTransition {
    /// Restart from the currently visible pose, including an interrupted blend.
    pub fn start(&mut self, live: &CameraPose) {
        self.from = *live;
        self.timer = 0.0;
    }

    /// Advance the cubic ease and return the live pose for the sole camera writer.
    pub fn advance(&mut self, target: &CameraPose, dt: f32) -> CameraPose {
        self.timer = (self.timer + dt * self.speed).min(1.0);
        if self.timer < 1.0 {
            // A focus-aware blend can swing the eye toward a distant orbit pivot
            // during first-person handoff. Blend eye and orientation directly.
            let u = 1.0 - self.timer.clamp(0.0, 1.0);
            CameraPose::lerp_direct(&self.from, target, 1.0 - u * u * u)
        } else {
            *target
        }
    }
}
