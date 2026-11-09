//! View-dependent visibility fields: hide or fade Gaussians that were never observed from about
//! the current viewing direction, or display that uncertainty.

pub mod field;
pub mod render;

pub use field::{FieldError, VisibilityField, sh_basis4, visibility_from_evidence};
pub use render::GaussianVisibility;

/// What the cloud draws when a visibility field is attached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum VisibilityDisplay {
    /// Field ignored (regular render; nothing skipped).
    Off,
    /// Opacity multiplied by `keep(vis)`; Gaussians with `keep == 0` are skipped.
    #[default]
    Faded,
    /// Only what `Faded` removes: opacity multiplied by `1 - keep(vis)`.
    Removed,
    /// Colour replaced by a turbo map of `1 - vis` (red = not observed from here).
    Uncertainty,
    /// Colour blended towards the uncertainty colour by `overlay * (1 - vis)`.
    Overlay,
}

impl VisibilityDisplay {
    pub const ALL: [Self; 5] = [Self::Off, Self::Faded, Self::Removed, Self::Uncertainty, Self::Overlay];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Faded => "faded",
            Self::Removed => "removed only",
            Self::Uncertainty => "uncertainty",
            Self::Overlay => "overlay",
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Shape of `keep(vis)` between `lo` and `hi`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum VisibilityCurve {
    /// `keep = vis >= (lo + hi) / 2`: hard cut, everything below is skipped.
    Hide,
    /// `keep = smoothstep(u)^gamma`, `u = clamp((vis - lo) / (hi - lo))`.
    #[default]
    Smooth,
    /// `keep = u^gamma`.
    Linear,
}

impl VisibilityCurve {
    pub const ALL: [Self; 3] = [Self::Hide, Self::Smooth, Self::Linear];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hide => "hide",
            Self::Smooth => "smooth",
            Self::Linear => "linear",
        }
    }
}

/// Runtime controls, uploaded as a uniform every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct VisibilitySettings {
    pub display: VisibilityDisplay,
    pub curve: VisibilityCurve,
    /// Below `lo` a Gaussian is fully removed (and skipped by the sort).
    pub lo: f32,
    /// Above `hi` a Gaussian is fully kept.
    pub hi: f32,
    /// Curve exponent; > 1 fades harder.
    pub gamma: f32,
    /// Relative evidence (share of the best direction) for full visibility. `None`: the field's.
    pub rho: Option<f32>,
    /// Absolute evidence gain (`1 - exp(-gain v)`).
    pub gain: f32,
    /// Overlay strength for `VisibilityDisplay::Overlay`.
    pub overlay: f32,
    /// Weight of the fit-residual term: visibility is capped at `1 - fit * fit_resid_normalised`
    /// (0: unseen-direction evidence only).
    #[serde(default)]
    pub fit: f32,
}

impl Default for VisibilitySettings {
    fn default() -> Self {
        Self {
            display: VisibilityDisplay::Faded,
            curve: VisibilityCurve::Smooth,
            lo: 0.3,
            hi: 0.7,
            gamma: 1.0,
            rho: None,
            gain: 1.0,
            overlay: 0.8,
            fit: 0.0,
        }
    }
}

/// Opacity below which a faded Gaussian is skipped instead of drawn.
pub const KEEP_EPSILON: f32 = 1.0 / 255.0;

impl VisibilitySettings {
    /// CPU reference of the GPU curve: opacity multiplier for visibility `vis`.
    pub fn keep(&self, vis: f32) -> f32 {
        let (lo, hi) = (self.lo.min(self.hi), self.lo.max(self.hi));
        match self.curve {
            VisibilityCurve::Hide => {
                if vis >= 0.5 * (lo + hi) {
                    1.0
                } else {
                    0.0
                }
            }
            VisibilityCurve::Smooth | VisibilityCurve::Linear => {
                if hi - lo < 1e-6 {
                    return if vis >= lo { 1.0 } else { 0.0 };
                }
                let u = ((vis - lo) / (hi - lo)).clamp(0.0, 1.0);
                let s = if self.curve == VisibilityCurve::Smooth { u * u * (3.0 - 2.0 * u) } else { u };
                s.powf(self.gamma.max(1e-3))
            }
        }
    }
}

