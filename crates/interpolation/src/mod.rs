//! Bounded, frame-driven playback for cursors, 3D positions and orientations.
//! Transport/session ownership stays with callers. Hold a [`Playback`] in an ECS
//! component or any other owner; sample on each rendered frame while active.
//! [`Interpolate`] supplies the value-specific curve; the queue knows no geometry.
mod value;
use std::{collections::VecDeque, time::Duration};
pub use value::Interpolate;

const MAX_FUTURE_UPDATES: usize = 2;

/// Opaque stream identity and accepted sample order supplied by the adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleStamp {
    pub stream: u64,
    pub sequence: u64,
    /// Observation time in the receiving application's Time<Real> timeline.
    pub received_at: Duration,
}
#[derive(Clone, Copy, Debug)]
struct Knot<T> {
    value: T,
    /// Controls for the segment ending at this knot, as in the TS reference.
    control0: Option<T>,
    control1: Option<T>,
}
impl<T: Interpolate> Knot<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            control0: None,
            control1: None,
        }
    }
    fn curve_from(self, start: T) -> T::Curve {
        T::curve(start, self.value, [self.control0, self.control1])
    }
}
#[derive(Debug)]
struct Segment<T: Interpolate> {
    started: Duration,
    // Frozen on start: later lookahead must not reshape an in-flight curve.
    curve: T::Curve,
}
/// Timing policy belongs to the consumer, not to the transport.
#[derive(Clone, Copy, Debug)]
pub struct PlaybackSettings {
    pub segment_duration: Duration,
    pub stall_reset: Duration,
}
impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            segment_duration: Duration::from_millis(80),
            stall_reset: Duration::from_millis(300),
        }
    }
}

/// Reference-style segment queue: cubic/quadratic/linear for positions, SLERP for
/// rotations. Keeps the active segment when skipping a backlog; never extrapolates.
#[derive(Debug)]
pub struct Playback<T: Interpolate> {
    settings: PlaybackSettings,
    // At most two past knots, the current target, and two future targets.
    knots: VecDeque<Knot<T>>,
    current: usize,
    active: Option<Segment<T>>,
    stamp: Option<SampleStamp>,
    displayed: T,
    rendered_at: Option<Duration>,
}
impl<T: Interpolate> Default for Playback<T> {
    fn default() -> Self {
        Self::new(PlaybackSettings::default())
    }
}
impl<T: Interpolate> Playback<T> {
    /// Panics for zero durations, which cannot define a finite playback interval.
    pub fn new(settings: PlaybackSettings) -> Self {
        assert!(!settings.segment_duration.is_zero() && !settings.stall_reset.is_zero());
        Self {
            settings,
            knots: VecDeque::new(),
            current: 0,
            active: None,
            stamp: None,
            displayed: T::default(),
            rendered_at: None,
        }
    }
    /// Drop all trajectory/order history for hide, removal or coordinate changes.
    /// Retains this consumer's timing settings.
    pub fn clear(&mut self) {
        *self = Self::new(self.settings);
    }
    /// Number of retained knots, including at most two past knots. Never above five.
    pub fn buffered_samples(&self) -> usize {
        self.knots.len()
    }
    /// Accept an ordered sample stamped on the receiving application's monotonic
    /// clock. A new opaque stream starts immediately. Rejects invalid values,
    /// duplicate/obsolete sequences, backwards observation times and future stamps.
    pub fn push(&mut self, stamp: SampleStamp, value: T, now: Duration) {
        let Some(value) = value.validated() else {
            return;
        };
        if stamp.received_at > now {
            return;
        }
        if let Some(old) = self.stamp
            && stamp.stream == old.stream
            && (stamp.sequence <= old.sequence || stamp.received_at < old.received_at)
        {
            return;
        }
        let reset = self.stamp.is_none_or(|old| {
            old.stream != stamp.stream
                || stamp.received_at.saturating_sub(old.received_at) > self.settings.stall_reset
        }) || self
            .rendered_at
            .is_some_and(|last| now.saturating_sub(last) > self.settings.stall_reset);
        self.stamp = Some(stamp);
        if reset {
            self.restart(value, now);
            return;
        }
        let mut knot = Knot::new(value);
        let len = self.knots.len();
        if len >= 2
            && let Some([incoming, outgoing]) =
                T::controls(self.knots[len - 2].value, self.knots[len - 1].value, value)
        {
            self.knots[len - 1].control1 = Some(incoming);
            knot.control0 = Some(outgoing);
        }
        self.knots.push_back(knot);
        if self.knots.len().saturating_sub(self.current + 1) > MAX_FUTURE_UPDATES {
            // Reference fast-forward: finish the current segment, then go straight
            // to the newest target. Its discarded neighbors' controls are invalid.
            self.knots.truncate(self.current + 1);
            self.knots.push_back(Knot::new(value));
        }
    }
    fn restart(&mut self, value: T, now: Duration) {
        self.knots.clear();
        self.knots.push_back(Knot::new(value));
        self.current = 1;
        self.active = None;
        self.displayed = value;
        self.rendered_at = Some(now);
    }
    /// Returns the displayed value and whether further frames are needed.
    /// Before the first valid sample (or after clear), returns `(T::default(), false)`.
    /// Call every rendered frame while active; no background timer is installed.
    pub fn sample(&mut self, now: Duration) -> (T, bool) {
        let stalled = self
            .rendered_at
            .is_some_and(|last| now.saturating_sub(last) > self.settings.stall_reset);
        self.rendered_at = Some(now);
        if stalled && let Some(latest) = self.knots.back() {
            self.restart(latest.value, now);
        }
        let mut started = now;
        while let Some(target) = self.knots.get(self.current).copied() {
            let start = self.knots[self.current - 1].value;
            let active = self.active.get_or_insert_with(|| Segment {
                started,
                curve: target.curve_from(start),
            });
            let elapsed = now.saturating_sub(active.started);
            if elapsed < self.settings.segment_duration {
                let t = elapsed.as_secs_f32() / self.settings.segment_duration.as_secs_f32();
                self.displayed = if elapsed.is_zero() {
                    start
                } else {
                    T::sample_curve(&active.curve, t)
                };
                return (self.displayed, true);
            }
            // Snap to the exact endpoint, avoiding polynomial roundoff. Carry the
            // planned end time into lookahead rather than adding a frame per knot.
            self.displayed = target.value;
            started = active.started + self.settings.segment_duration;
            self.active = None;
            if self.knots.len() >= 3 {
                self.knots.pop_front();
            } else {
                self.current += 1;
            }
        }
        (self.displayed, false)
    }
}

#[cfg(test)]
mod tests;
