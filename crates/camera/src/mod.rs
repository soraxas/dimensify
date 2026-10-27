//! Shared camera pose, view requests, projection conventions and scene bounds.
//! Camera values use Bevy's right-handed Y-up frame. Convert canonical Z-up
//! asset data at ingestion, once. Consumers retain their camera writer.
mod bounds;
mod fov;
mod motion;
mod orbit;
mod pose;
mod touch;
mod transition;
mod walk;

pub use bounds::{FitExtent, fit_orbit, perspective_fit_distance, scene_bounding_sphere};
pub use fov::FovConvention;
pub use motion::{FlyInput, fly_velocity, smooth_look};
pub use orbit::OrbitState;
pub use pose::{
    CameraPose, CameraPoseCurve, forward_from_yaw_pitch, shortest_angle, yaw_pitch_from_dir,
};
pub use touch::{TouchGestureFrame, TouchMotionTracker, TouchSample};
pub use transition::{PoseTransition, TargetPose};
pub use walk::{
    WALK_HOVER_HEIGHT, WALK_MAX_FRAME_DELTA, WalkBody, WalkEnvironment, WalkInput, WalkMotion,
    WalkMove, WalkSupport,
};

/// Clamp a vertical lens to projection-safe limits; invalid input uses 45 degrees.
pub fn clamp_vertical_fov(fov: f32) -> f32 {
    if fov.is_finite() {
        fov.clamp(1.7_f32.to_radians(), 165.0_f32.to_radians())
    } else {
        std::f32::consts::FRAC_PI_4
    }
}

#[cfg(feature = "inspect")]
mod inspect;
#[cfg(feature = "inspect")]
pub use inspect::{
    InspectContext, InspectController, InspectDrag, InspectEnvironment, InspectFrame,
    InspectGesture, InspectHit, InspectInput, InspectMotion, clip_ray_interval,
    inspect_view_anchor, inspect_wheel_input, pointer_drag_owned,
};

/// Optional device-to-camera adapter for graphical consumers.
#[cfg(feature = "viewer")]
pub mod viewer;
