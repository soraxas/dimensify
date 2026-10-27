//! Viewport-owned touch tracking, independent of device polling or UI widgets.
use bevy::prelude::*;

/// One active contact in logical viewport pixels, supplied by the host adapter.
#[derive(Clone, Copy, Debug)]
pub struct TouchSample {
    pub id: u64,
    pub position: Vec2,
    pub start_position: Vec2,
    pub just_started: bool,
}

/// Motion between samples of the same fingers; membership changes rebase it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TouchGestureFrame {
    #[default]
    None,
    OneFinger {
        motion: Vec2,
    },
    TwoFinger {
        motion: Vec2,
        pinch: f32,
    },
}

/// Stable contact identity and ownership for one- and two-finger camera gestures.
#[derive(Default)]
pub struct TouchMotionTracker {
    current: (Option<TouchSample>, Option<TouchSample>),
    previous: (Option<TouchSample>, Option<TouchSample>),
    owned: Vec<u64>,
}

impl TouchMotionTracker {
    /// Cancel motion and ownership; held contacts cannot reacquire without a new start.
    pub fn clear(&mut self) {
        self.owned.clear();
        self.reset_samples();
    }

    fn reset_samples(&mut self) {
        self.current = (None, None);
        self.previous = (None, None);
    }

    /// Admit new contacts inside the host's viewport, then compute stable motion.
    /// Suppression, missing viewport or a third finger cancels ownership. Contacts
    /// started outside the viewport cannot become half of a camera pinch.
    pub fn update_owned(&mut self, samples: &[TouchSample], allowed: bool, viewport: Option<Rect>) {
        let Some(viewport) = viewport.filter(|_| allowed && samples.len() <= 2) else {
            self.clear();
            return;
        };
        self.owned.retain(|id| {
            samples.iter().any(|sample| {
                sample.id == *id && !sample.just_started && viewport.contains(sample.position)
            })
        });
        for sample in samples {
            if sample.just_started
                && viewport.contains(sample.start_position)
                && viewport.contains(sample.position)
            {
                self.owned.push(sample.id);
            }
        }
        if samples
            .iter()
            .any(|sample| !self.owned.contains(&sample.id))
        {
            self.reset_samples();
            return;
        }
        if samples.iter().any(|sample| sample.just_started) {
            self.reset_samples();
        }
        // Hash-map iteration order must not change gesture identity or its anchor.
        let mut active = samples.to_vec();
        active.sort_unstable_by_key(|sample| sample.id);
        self.previous = self.current;
        self.current = (active.first().copied(), active.get(1).copied());
        if active.is_empty() {
            self.reset_samples();
        }
    }

    /// Current owned contacts in canonical ID order, for gesture identity and pivot.
    pub fn current(&self) -> (Option<TouchSample>, Option<TouchSample>) {
        self.current
    }

    /// Logical drag, midpoint pan and distance change, without application gain.
    pub fn gesture(&self) -> TouchGestureFrame {
        match (self.current, self.previous) {
            ((Some(current), None), (Some(previous), None)) if current.id == previous.id => {
                TouchGestureFrame::OneFinger {
                    motion: current.position - previous.position,
                }
            }
            ((Some(a), Some(b)), (Some(previous_a), Some(previous_b)))
                if a.id == previous_a.id && b.id == previous_b.id =>
            {
                TouchGestureFrame::TwoFinger {
                    motion: a.position.midpoint(b.position)
                        - previous_a.position.midpoint(previous_b.position),
                    pinch: a.position.distance(b.position)
                        - previous_a.position.distance(previous_b.position),
                }
            }
            _ => TouchGestureFrame::None,
        }
    }
}
