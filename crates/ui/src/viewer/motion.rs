//! Reversible 200 ms transitions shared by panels and disclosures.

/// A reversible transition with smoothstep easing and a fixed full travel time.
#[cfg_attr(feature = "viewer", derive(bevy::prelude::Component))]
pub struct Reveal(f32);

impl Reveal {
    /// Start at the settled open or closed position.
    pub fn new(open: bool) -> Self {
        Self(if open { 1. } else { 0. })
    }

    /// Advance toward a target; immediate transitions also work with zero delta.
    pub fn advance(&mut self, open: bool, delta: f32, immediate: bool) -> f32 {
        let target = if open { 1. } else { 0. };
        if immediate {
            self.0 = target;
        } else {
            let distance = delta.max(0.) / 0.2;
            self.0 += (target - self.0).clamp(-distance, distance);
        }
        self.fraction()
    }

    /// Current eased fraction in the inclusive range 0..=1.
    pub fn fraction(&self) -> f32 {
        self.0 * self.0 * (3. - 2. * self.0)
    }
}

/// Attach to a flex panel node to enable retained show/hide motion.
#[cfg_attr(feature = "viewer", derive(bevy::prelude::Component))]
#[cfg_attr(
    feature = "viewer",
    require(bevy::prelude::UiTransform, super::ResponsivePanel)
)]
pub struct PanelMotion {
    open: bool,
    reveal: Reveal,
}

impl PanelMotion {
    /// Start settled. The viewer plugin synchronizes display on its next update.
    pub fn new(open: bool) -> Self {
        Self {
            open,
            reveal: Reveal::new(open),
        }
    }

    /// Change the target without restarting a transition already in progress.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Whether the panel currently targets its open position.
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub(super) fn advance(&mut self, delta: f32, immediate: bool) -> f32 {
        self.reveal.advance(self.open, delta, immediate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_reverses_without_jumping_or_restarting() {
        let mut panel = PanelMotion::new(false);
        panel.set_open(true);
        assert!(panel.is_open());
        assert_eq!(panel.advance(0.1, false), 0.5);
        panel.set_open(false);
        assert_eq!(panel.advance(0., false), 0.5);
        let closing = panel.advance(0.02, false);
        assert!(closing > 0. && closing < 0.5);
        panel.set_open(true);
        assert_eq!(panel.advance(0., false), closing);
        assert_eq!(panel.advance(1., false), 1.);
        panel.set_open(false);
        assert_eq!(panel.advance(1., false), 0.);
    }

    #[test]
    fn reduced_motion_settles_mid_transition_with_zero_delta() {
        let mut panel = PanelMotion::new(false);
        panel.set_open(true);
        assert!(panel.is_open());
        assert_eq!(panel.advance(0.1, false), 0.5);
        assert_eq!(panel.advance(0., true), 1.);
        panel.set_open(false);
        assert_eq!(panel.advance(0., true), 0.);
    }

    #[test]
    fn reveal_reverses_and_ignores_negative_time() {
        let mut reveal = Reveal::new(false);
        assert_eq!(reveal.advance(true, 0.1, false), 0.5);
        assert_eq!(reveal.advance(false, -1., false), 0.5);
        let closing = reveal.advance(false, 0.02, false);
        assert!(closing > 0. && closing < 0.5);
        assert_eq!(reveal.advance(true, 0., true), 1.);
        assert_eq!(reveal.advance(false, 0., true), 0.);
    }
}
