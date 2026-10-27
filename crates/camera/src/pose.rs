//! [`CameraPose`] - the single source of truth for the camera (supersplat's
//! `Camera`). Every mode writes a *target* pose; the manager eases the live pose
//! toward it and syncs it to the [`Transform`]. The focus-aware [`CameraPose::lerp`]
//! is what makes cross-mode transitions smooth: it interpolates the look-*target*,
//! not just yaw/pitch, so the camera arcs naturally instead of snapping.

use bevy::prelude::*;
use interpolation::Interpolate;

/// Buffered camera segment: Bezier translation, SLERP rotation and linear lens values.
#[derive(Debug)]
pub struct CameraPoseCurve {
    position: <Vec3 as Interpolate>::Curve,
    rotation: <Quat as Interpolate>::Curve,
    endpoints: [CameraPose; 2],
}

impl Interpolate for CameraPose {
    type Curve = CameraPoseCurve;

    fn validated(self) -> Option<Self> {
        let position = self.position.validated()?;
        let rotation = self.rotation().validated()?;
        (self.distance.is_finite()
            && self.distance > 0.
            && self.fov.is_finite()
            && self.fov > 0.
            && self.fov < std::f32::consts::PI)
            .then(|| Self::from_rotation(position, rotation, self.distance, self.fov))
    }

    fn controls(previous: Self, middle: Self, next: Self) -> Option<[Self; 2]> {
        Some(
            Vec3::controls(previous.position, middle.position, next.position)?
                .map(|position| Self { position, ..middle }),
        )
    }

    fn curve(start: Self, end: Self, controls: [Option<Self>; 2]) -> Self::Curve {
        CameraPoseCurve {
            position: Vec3::curve(
                start.position,
                end.position,
                controls.map(|p| p.map(|p| p.position)),
            ),
            rotation: Quat::curve(start.rotation(), end.rotation(), [None; 2]),
            endpoints: [start, end],
        }
    }

    fn sample_curve(curve: &Self::Curve, t: f32) -> Self {
        let [start, end] = curve.endpoints;
        Self::from_rotation(
            Vec3::sample_curve(&curve.position, t),
            Quat::sample_curve(&curve.rotation, t),
            start.distance + (end.distance - start.distance) * t,
            start.fov + (end.fov - start.fov) * t,
        )
    }
}

/// A camera pose as eye position + orientation + orbit distance + fov.
/// `focus = position + forward * distance`.
#[derive(Resource, Clone, Copy, Debug)]
pub struct CameraPose {
    pub position: Vec3,
    /// Yaw around +Y (radians).
    pub yaw: f32,
    /// Pitch (radians), + looks up.
    pub pitch: f32,
    /// Full camera orientation. Unlike yaw/pitch, this preserves authored roll.
    pub orientation: Quat,
    /// Distance from `position` to the look-at focus point.
    pub distance: f32,
    /// Vertical field of view (radians).
    pub fov: f32,
}

impl Default for CameraPose {
    fn default() -> Self {
        let yaw = 0.0;
        let pitch = 0.0;
        Self {
            position: Vec3::new(20.0, 20.0, 20.0),
            yaw,
            pitch,
            orientation: rotation_from_yaw_pitch(yaw, pitch),
            distance: 1.0,
            fov: std::f32::consts::FRAC_PI_4,
        }
    }
}

impl CameraPose {
    /// Forward (look) direction from yaw/pitch.
    pub fn forward(&self) -> Vec3 {
        self.orientation * Vec3::NEG_Z
    }

    /// World-space look-at point.
    pub fn focus(&self) -> Vec3 {
        self.position + self.forward() * self.distance
    }

    /// Build a pose looking from `eye` toward `target`.
    pub fn looking(eye: Vec3, target: Vec3, fov: f32) -> Self {
        let dir = target - eye;
        let distance = dir.length().max(1e-4);
        let direction = dir / distance;
        let (yaw, pitch) = yaw_pitch_from_dir(direction);
        Self {
            position: eye,
            yaw,
            pitch,
            orientation: Transform::IDENTITY.looking_to(direction, Vec3::Y).rotation,
            distance,
            fov,
        }
    }

    /// Build a pose from an authored full quaternion, preserving camera roll.
    pub fn from_rotation(position: Vec3, orientation: Quat, distance: f32, fov: f32) -> Self {
        let orientation = orientation.normalize();
        let forward = orientation * Vec3::NEG_Z;
        let horizontal = forward.xz().length();
        // At a vertical pole forward has no heading. Floating-point noise in
        // its X/Z components can otherwise reverse yaw when Look hands off to
        // an upright controller. The screen-right axis still defines heading;
        // preserve it at the pole, including any authored roll folded into yaw.
        let yaw = if horizontal <= 1e-6 {
            let right = orientation * Vec3::X;
            (-right.z).atan2(right.x)
        } else {
            (-forward.x).atan2(-forward.z)
        };
        // asin(y) loses near-pole precision when y rounds to +/-1.
        let pitch = forward.y.atan2(horizontal);
        Self {
            position,
            yaw,
            pitch,
            orientation,
            distance,
            fov,
        }
    }

    /// Full normalized orientation, including authored roll.
    pub fn rotation(&self) -> Quat {
        self.orientation
    }

    /// Bevy transform for this pose (Y-up).
    pub fn to_transform(&self) -> Transform {
        Transform::from_translation(self.position).with_rotation(self.orientation)
    }

    /// Direct interpolation: lerp eye, distance and FOV; shortest-path SLERP
    /// orientation. Used for **mode transitions** - a clean dolly+rotate with
    /// no focus arc (unlike [`CameraPose::lerp`], which swings the look-target
    /// and is only right for orbit-style re-framing). `t` is pre-eased.
    pub fn lerp_direct(a: &CameraPose, b: &CameraPose, t: f32) -> CameraPose {
        // Controllers seed from these angles when a transition is interrupted.
        // Independently lerping Euler angles does not match the rendered SLERP.
        Self::from_rotation(
            a.position.lerp(b.position, t),
            shortest_slerp(a.orientation, b.orientation, t),
            a.distance + (b.distance - a.distance) * t,
            a.fov + (b.fov - a.fov) * t,
        )
    }

    /// Focus-aware interpolation: lerp eye **and** focus, then re-derive
    /// distance/angles. `t` should already be eased by the caller.
    pub fn lerp(a: &CameraPose, b: &CameraPose, t: f32) -> CameraPose {
        let position = a.position.lerp(b.position, t);
        let focus = a.focus().lerp(b.focus(), t);
        let dir = focus - position;
        let distance = dir.length().max(1e-4);
        let (yaw, pitch) = yaw_pitch_from_dir(dir / distance);
        CameraPose {
            position,
            yaw,
            pitch,
            orientation: Transform::IDENTITY
                .looking_to(dir / distance, Vec3::Y)
                .rotation,
            distance,
            fov: a.fov + (b.fov - a.fov) * t,
        }
    }
}

fn rotation_from_yaw_pitch(yaw: f32, pitch: f32) -> Quat {
    Transform::IDENTITY
        .looking_to(forward_from_yaw_pitch(yaw, pitch), Vec3::Y)
        .rotation
}

fn shortest_slerp(a: Quat, mut b: Quat, t: f32) -> Quat {
    if a.dot(b) < 0.0 {
        b = -b;
    }
    a.slerp(b, t)
}

/// Wrap an angle delta into `[-pi, pi]` for shortest-path interpolation.
pub fn shortest_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

/// Forward direction from yaw (around +Y) and pitch.
pub fn forward_from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(-sy * cp, sp, -cy * cp)
}

/// Inverse of [`forward_from_yaw_pitch`] for a unit direction.
pub fn yaw_pitch_from_dir(dir: Vec3) -> (f32, f32) {
    let yaw = (-dir.x).atan2(-dir.z);
    let pitch = dir.y.clamp(-1.0, 1.0).asin();
    (yaw, pitch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_interpolation_preserves_roll_and_uses_shortest_quaternion_path() {
        let a = CameraPose::from_rotation(Vec3::ZERO, Quat::from_rotation_z(0.4), 1.0, 0.7);
        let b_rotation = -Quat::from_rotation_z(0.8);
        let b = CameraPose::from_rotation(Vec3::X, b_rotation, 1.0, 0.7);

        let halfway = CameraPose::lerp_direct(&a, &b, 0.5);

        assert!(
            halfway
                .rotation()
                .abs_diff_eq(Quat::from_rotation_z(0.6), 1.0e-5)
        );
    }
}
