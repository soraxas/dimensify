//! Shared inspection view model and explicit application view requests.
//! Manipulation is owned by `inspect`; this resource publishes its focus/lens
//! for UI and playback, and carries one-shot fit/focus/preset requests.
use super::pose::CameraPose;
use bevy::prelude::*;
const MIN_DISTANCE: f32 = 0.01;

/// Shared view model. Application callers edit targets, then call `request_view`.
/// Inspect consumes that request once and publishes the result. It does not
/// continuously damp these fields or use them as a competing camera controller.
#[derive(Resource)]
pub struct OrbitState {
    /// Semantic view request counter. Inspect consumes each request once.
    pub revision: u64,
    /// Explicit viewcube snap; ordinary view requests retain upright yaw/pitch.
    pub requested_orientation: Option<Quat>,
    pub focus: Vec3,
    pub target_focus: Vec3,
    pub yaw: f32,
    pub target_yaw: f32,
    pub pitch: f32,
    pub target_pitch: f32,
    pub distance: f32,
    pub target_distance: f32,
    pub fov: f32,
    pub target_fov: f32,
    /// When true, scroll wheel adjusts FOV (telephoto/wide-angle lens zoom)
    /// rather than orbit distance (dolly).
    pub zoom_by_fov: bool,
    /// Switch the main 3D camera to orthographic projection.
    pub use_ortho: bool,
    /// Current orthographic scale (world-units visible per pixel, damped).
    pub ortho_scale: f32,
    pub target_ortho_scale: f32,
}

impl Default for OrbitState {
    fn default() -> Self {
        let default_fov = std::f32::consts::FRAC_PI_4;
        Self {
            revision: 0,
            requested_orientation: None,
            focus: Vec3::ZERO,
            target_focus: Vec3::ZERO,
            yaw: 0.0,
            target_yaw: 0.0,
            pitch: 0.3,
            target_pitch: 0.3,
            distance: 2.0,
            target_distance: 2.0,
            fov: default_fov,
            target_fov: default_fov,
            zoom_by_fov: false,
            use_ortho: false,
            ortho_scale: 0.01,
            target_ortho_scale: 0.01,
        }
    }
}

impl OrbitState {
    /// Current semantic request revision; pose publication does not advance it.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Commit an application focus/fit/view/lens request after editing its targets.
    /// This resource is the compatibility-facing view model, not a controller.
    pub fn request_view(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.requested_orientation = None;
    }

    /// Request a full orientation, preserving roll and rejecting invalid quaternions.
    pub fn request_orientation(&mut self, rotation: Quat) {
        if !rotation.is_finite() || rotation.length_squared() < 1e-6 {
            return;
        }
        self.request_view();
        let rotation = rotation.normalize();
        let pose =
            CameraPose::from_rotation(Vec3::ZERO, rotation, self.target_distance, self.target_fov);
        self.target_yaw = pose.yaw;
        self.target_pitch = pose.pitch;
        self.requested_orientation = Some(rotation);
    }

    /// Resolve the requested focus, orientation, distance and lens into a pose.
    pub fn requested_pose(&self) -> CameraPose {
        let rotation = self.requested_orientation.unwrap_or_else(|| {
            Quat::from_rotation_y(self.target_yaw) * Quat::from_rotation_x(self.target_pitch)
        });
        CameraPose::from_rotation(
            self.target_focus - rotation * Vec3::NEG_Z * self.target_distance,
            rotation,
            self.target_distance,
            self.target_fov,
        )
    }

    /// Publish target fields after consuming the current semantic request.
    pub fn accept_requested_view(&mut self) {
        self.focus = self.target_focus;
        self.yaw = self.target_yaw;
        self.pitch = self.target_pitch;
        self.distance = self.target_distance;
        self.fov = self.target_fov;
        self.ortho_scale = self.target_ortho_scale;
    }

    /// Publish manipulation output without issuing a new semantic request.
    pub fn publish_pose(&mut self, pose: &CameraPose) {
        self.requested_orientation = None;
        self.yaw = pose.yaw;
        self.target_yaw = pose.yaw;
        self.pitch = pose.pitch;
        self.target_pitch = pose.pitch;
        self.distance = pose.distance;
        self.target_distance = pose.distance;
        self.focus = pose.focus();
        self.target_focus = self.focus;
        self.fov = pose.fov;
        self.target_fov = pose.fov;
    }

    /// Seed both current and target from a pose (used on entering Orbit so the
    /// pivot picks up exactly where the previous mode left off - no snap).
    pub fn seed_from(&mut self, pose: &CameraPose) {
        self.request_view();
        self.yaw = pose.yaw;
        self.target_yaw = pose.yaw;
        self.pitch = pose.pitch;
        self.target_pitch = pose.pitch;
        // Adopt the dolly floor before any input. Move the synthetic pivot,
        // not the eye: otherwise the first zoom-in from a close Look/authored
        // pose increases its radius and sends the camera backwards.
        self.distance = pose.distance.max(MIN_DISTANCE);
        self.target_distance = self.distance;
        self.focus = pose.position + (self.rotation() * Vec3::NEG_Z) * self.distance;
        self.target_focus = self.focus;
        self.fov = pose.fov;
        self.target_fov = pose.fov;
    }

    fn rotation(&self) -> Quat {
        // Unlike looking_to(forward, Y), yaw remains defined when forward is
        // parallel to world-up. This preserves screen axes at both orbit poles.
        Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch)
    }
}
