//! Geometry policies, separate from queue timing and stream lifecycle.
use bevy_math::{Quat, Vec2, Vec3, cubic_splines::CubicSegment};
use std::fmt::Debug;

/// Implement this for a sample value (including domain newtypes) to choose how
/// it is validated and interpolated. No arithmetic is imposed by the queue.
/// `Vec2`/`Vec3` use Bézier controls; `Quat` uses spherical interpolation.
pub trait Interpolate: Copy + Default + Debug {
    type Curve: Debug;
    /// Reject invalid input or return its canonical form before buffering.
    fn validated(self) -> Option<Self>;
    /// Optional incoming/outgoing controls around the middle of three samples.
    /// Returning None supports types whose segments do not use Bézier controls.
    fn controls(_previous: Self, _middle: Self, _next: Self) -> Option<[Self; 2]> {
        None
    }
    /// Build an immutable segment. Controls are ordered start-side, end-side.
    fn curve(start: Self, end: Self, controls: [Option<Self>; 2]) -> Self::Curve;
    /// Evaluate at `t` in [0, 1]. The queue assigns exact validated endpoints.
    fn sample_curve(curve: &Self::Curve, t: f32) -> Self;
}

// The TS example links this distance-weighted construction but omits its helper:
// https://scaledinnovation.com/analytics/splines/aboutSplines.html
const TENSION: f32 = 0.5;
macro_rules! position_value {
    ($value:ty) => {
        impl Interpolate for $value {
            type Curve = CubicSegment<Self>;
            fn validated(self) -> Option<Self> {
                self.is_finite().then_some(self)
            }
            fn controls(previous: Self, middle: Self, next: Self) -> Option<[Self; 2]> {
                let before = previous.distance(middle);
                let after = middle.distance(next);
                let total = before + after;
                if !total.is_finite() {
                    return None;
                }
                if total <= f32::EPSILON {
                    return Some([middle; 2]);
                }
                let tangent = (next - previous) * TENSION;
                Some([
                    middle - tangent * (before / total),
                    middle + tangent * (after / total),
                ])
            }
            fn curve(start: Self, end: Self, controls: [Option<Self>; 2]) -> Self::Curve {
                let (c0, c1) = match controls {
                    [Some(c0), Some(c1)] => (c0, c1),
                    // Exact degree elevation, not a different quadratic approximation.
                    [Some(c), None] | [None, Some(c)] => {
                        (start.lerp(c, 2. / 3.), end.lerp(c, 2. / 3.))
                    }
                    [None, None] => (start.lerp(end, 1. / 3.), start.lerp(end, 2. / 3.)),
                };
                CubicSegment::new_bezier([start, c0, c1, end])
            }
            fn sample_curve(curve: &Self::Curve, t: f32) -> Self {
                curve.position(t)
            }
        }
    };
}
position_value!(Vec2);
position_value!(Vec3);

impl Interpolate for Quat {
    type Curve = [Quat; 2];
    fn validated(self) -> Option<Self> {
        let length = self.length_squared();
        (self.is_finite() && length.is_finite() && length > f32::EPSILON).then(|| self.normalize())
    }
    fn curve(start: Self, end: Self, _controls: [Option<Self>; 2]) -> Self::Curve {
        [start, end]
    }
    fn sample_curve(curve: &Self::Curve, t: f32) -> Self {
        // Bevy/glam SLERP chooses the short rotation, including q versus -q.
        // Never apply vector Bézier arithmetic to quaternion components.
        curve[0].slerp(curve[1], t).normalize()
    }
}
