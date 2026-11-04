//! Absolute-time camera motion; sampling never accumulates frame deltas.
use crate::CameraPose;
use bevy::prelude::*;
use interpolation::Interpolate;
use std::time::Duration;

/// A bounded smooth camera move. Inputs are validated once; sampling is allocation-free.
#[derive(Clone, Debug)]
pub struct CameraPath {
    from: CameraPose,
    duration: Duration,
    motion: Motion,
}
#[derive(Clone, Debug)]
enum Motion {
    Between(CameraPose),
    Orbit { pivot: Vec3, radians: f32 },
}
fn ease(t: f32) -> f32 {
    // Quintic smoothstep: zero velocity and acceleration at both endpoints.
    (t * t * t * (t * (t * 6. - 15.) + 10.)).clamp(0., 1.)
}
impl CameraPath {
    fn checked(from: CameraPose, duration: Duration, motion: Motion) -> Result<Self, &'static str> {
        if duration.is_zero() || duration > Duration::from_secs(86_400) {
            return Err("Camera path duration must be positive and at most one day");
        }
        let from = from.validated().ok_or("Invalid camera path pose")?;
        Ok(Self {
            from,
            duration,
            motion,
        })
    }
    /// Ease eye, shortest-arc orientation and lens from one pose to another.
    pub fn between(
        from: CameraPose,
        to: CameraPose,
        duration: Duration,
    ) -> Result<Self, &'static str> {
        let to = to.validated().ok_or("Invalid camera path endpoint")?;
        Self::checked(from, duration, Motion::Between(to))
    }
    /// Orbit around world Y to an angular offset and return. Each half eases to rest.
    /// The original orientation/roll and lens are preserved relative to the pivot.
    pub fn orbit(
        from: CameraPose,
        pivot: Vec3,
        radians: f32,
        duration: Duration,
    ) -> Result<Self, &'static str> {
        if !pivot.is_finite() || !radians.is_finite() || radians.abs() > std::f32::consts::TAU {
            return Err("Invalid camera orbit pivot or angle");
        }
        Self::checked(from, duration, Motion::Orbit { pivot, radians })
    }
    pub fn duration(&self) -> Duration {
        self.duration
    }
    /// Sample an elapsed timestamp. Negative time is unrepresentable; times after
    /// completion hold the exact endpoint. Seeking does not depend on prior samples.
    pub fn sample(&self, elapsed: Duration) -> CameraPose {
        if elapsed.is_zero() {
            return self.from;
        }
        if elapsed >= self.duration {
            return match self.motion {
                Motion::Between(to) => to,
                Motion::Orbit { .. } => self.from,
            };
        }
        let t = (elapsed.as_secs_f64() / self.duration.as_secs_f64()) as f32;
        match self.motion {
            Motion::Between(to) => {
                let u = ease(t);
                if u >= 1. {
                    return to;
                }
                let mut pose = CameraPose::lerp_direct(&self.from, &to, u);
                // Convex weights avoid cancellation to zero for tiny positive
                // lens/distance endpoints and overflow in an endpoint difference.
                pose.position = self.from.position * (1. - u) + to.position * u;
                pose.distance = self.from.distance * (1. - u) + to.distance * u;
                pose.fov = self.from.fov * (1. - u) + to.fov * u;
                pose
            }
            Motion::Orbit { pivot, radians } => {
                let progress = if t <= 0.5 {
                    ease(t * 2.)
                } else {
                    ease((1. - t) * 2.)
                };
                let rotation = Quat::from_rotation_y(radians * progress);
                CameraPose::from_rotation(
                    pivot + rotation * (self.from.position - pivot),
                    rotation * self.from.rotation(),
                    self.from.distance,
                    self.from.fov,
                )
            }
        }
    }
}
