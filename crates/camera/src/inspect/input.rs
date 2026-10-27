//! Owned, logical-pixel input. No device polling or UI decisions in the controller.
use bevy::prelude::*;

/// Semantic movement independent of a mouse, touch device or widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectMotion {
    Orbit,
    Pan,
    Zoom,
}

/// Stable gesture identity; a changed identity rebases the anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectGesture {
    ViewCube(u64),
    Mouse(u64, InspectMotion),
    Touch(u64, Option<u64>),
    Wheel(InspectMotion),
}

impl InspectGesture {
    /// Return the manipulation implied by this gesture.
    pub(super) fn motion(self) -> InspectMotion {
        match self {
            Self::ViewCube(_) => InspectMotion::Orbit,
            Self::Mouse(_, motion) | Self::Wheel(motion) => motion,
            Self::Touch(_, None) => InspectMotion::Orbit,
            Self::Touch(_, Some(_)) => InspectMotion::Pan,
        }
    }
}

/// One frame of owned logical-pixel input.
#[derive(Clone, Copy, Debug, Default)]
pub struct InspectInput {
    pub gesture: Option<InspectGesture>,
    pub pointer: Option<Vec2>,
    /// Mouse and wheel are logical pixels; one-finger touch uses the same gain.
    pub delta: Vec2,
    /// Zoom in vendor pixels, positive toward the anchor.
    pub zoom: f32,
    pub cancel: bool,
}

/// Retain a viewport-owned button through its release frame.
/// A held button cannot acquire a drag after starting over captured UI.
pub fn pointer_drag_owned(
    owned: bool,
    pressed: bool,
    just_pressed: bool,
    just_released: bool,
    start_allowed: bool,
) -> bool {
    (pressed && (owned || (start_allowed && just_pressed))) || (just_released && owned)
}

/// Latched semantic mouse gesture, independent of UI and platform ownership.
#[derive(Default)]
pub struct InspectDrag {
    sequence: u64,
    pan: bool,
    left: bool,
    right: bool,
}
impl InspectDrag {
    /// Update resolved button ownership and latch the modifier on a new press.
    pub fn update_owned(
        &mut self,
        left: bool,
        right: bool,
        pan_modifier: bool,
    ) -> Option<InspectGesture> {
        if (left && !self.left) || (right && !self.right) {
            self.sequence = self.sequence.wrapping_add(1);
            self.pan = right || pan_modifier;
        }
        self.left = left;
        self.right = right;
        self.gesture()
    }

    /// Return the current identity; release both buttons to end the gesture.
    pub fn gesture(&self) -> Option<InspectGesture> {
        (self.left || self.right).then_some(InspectGesture::Mouse(
            self.sequence,
            if self.pan {
                InspectMotion::Pan
            } else {
                InspectMotion::Orbit
            },
        ))
    }
}

/// Convert wheel-line zoom and logical-pixel pan/orbit into inspect input.
/// Pan takes priority over orbit; zoom remains additive. Orbit may be locked.
pub fn inspect_wheel_input(
    pointer: Option<Vec2>,
    pan: Vec2,
    orbit: Vec2,
    zoom: f32,
    orbit_locked: bool,
) -> InspectInput {
    let mut input = InspectInput {
        pointer,
        zoom: zoom * 50.0,
        ..default()
    };
    if pan != Vec2::ZERO {
        input.gesture = Some(InspectGesture::Wheel(InspectMotion::Pan));
        input.delta = pan;
    } else if orbit != Vec2::ZERO && !orbit_locked {
        input.gesture = Some(InspectGesture::Wheel(InspectMotion::Orbit));
        input.delta = orbit;
    } else if zoom != 0.0 {
        input.gesture = Some(InspectGesture::Wheel(InspectMotion::Zoom));
    }
    input
}
