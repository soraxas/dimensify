//! Per-splat load effects evaluated by `render/effects.wesl`.
//!
//! The effect stage is compiled into the raster pipeline only while
//! [`SplatEffects::enabled`] is set; stages are then selected per frame by a
//! uniform mask, so composing or finishing effects never recompiles a pipeline.
use bevy::prelude::*;
use bevy_args::{Deserialize, Serialize};

/// Mask bits shared with `render/effects.wesl`.
pub const EFFECT_REVEAL: u32 = 1;
pub const EFFECT_EXPLOSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[serde(default)]
pub struct SplatEffects {
    /// Specialize the raster pipeline with the effect stage. Keep it set for as
    /// long as an effect may play: toggling it changes the pipeline key.
    pub enabled: bool,
    /// A glowing front sweeps outward from `origin`, settling splats behind it.
    pub reveal: bool,
    /// Splats burst out of `origin` along staggered arcs into place.
    pub explosion: bool,
    /// Seconds since the effect started.
    pub time: f32,
    /// World-space centre the effects radiate from.
    pub origin: Vec3,
    /// Farthest weighted distance from `origin`; normalizes the front and arcs.
    pub extent: f32,
    /// Per-axis weights for the reveal distance: `(1, 0, 1)` sweeps a disc
    /// across the ground plane, `Vec3::ONE` grows a sphere around `origin`.
    pub axes: Vec3,
    /// Nearest weighted distance from `origin` to the scene; the reveal front
    /// starts here rather than at the origin itself.
    pub near: f32,
    pub reveal_duration: f32,
    /// Front width as a fraction of `extent`.
    pub reveal_edge: f32,
    pub explosion_duration: f32,
    /// Linear glow colour; alpha is intensity.
    pub glow: Vec4,
}

impl Default for SplatEffects {
    fn default() -> Self {
        Self {
            enabled: false,
            reveal: false,
            explosion: false,
            time: 0.0,
            origin: Vec3::ZERO,
            extent: 1.0,
            axes: Vec3::new(1.0, 0.0, 1.0),
            near: 0.0,
            reveal_duration: 3.0,
            reveal_edge: 0.08,
            explosion_duration: 2.0,
            glow: Vec4::new(0.55, 0.75, 1.0, 1.5),
        }
    }
}

impl SplatEffects {
    /// Stages still animating at `time`. Finished stages drop out, so a
    /// completed effect renders exactly like the unanimated cloud.
    pub fn mask(&self) -> u32 {
        if !self.enabled {
            return 0;
        }
        let mut mask = 0;
        if self.reveal && self.time < self.reveal_duration {
            mask |= EFFECT_REVEAL;
        }
        if self.explosion && self.time < self.explosion_duration {
            mask |= EFFECT_EXPLOSION;
        }
        mask
    }

    /// Longest duration among the selected stages.
    pub fn duration(&self) -> f32 {
        let reveal = if self.reveal { self.reveal_duration } else { 0.0 };
        let explosion = if self.explosion {
            self.explosion_duration
        } else {
            0.0
        };
        reveal.max(explosion)
    }

    pub fn finished(&self) -> bool {
        self.mask() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_stages_drop_out_of_the_mask() {
        let mut effects = SplatEffects {
            enabled: true,
            reveal: true,
            explosion: true,
            reveal_duration: 3.0,
            explosion_duration: 2.0,
            ..default()
        };
        assert_eq!(effects.mask(), EFFECT_REVEAL | EFFECT_EXPLOSION);
        effects.time = 2.5;
        assert_eq!(effects.mask(), EFFECT_REVEAL);
        effects.time = 3.0;
        assert!(effects.finished());
        assert_eq!(effects.duration(), 3.0);
    }

    #[test]
    fn disabled_effects_never_animate() {
        let effects = SplatEffects {
            reveal: true,
            ..default()
        };
        assert_eq!(effects.mask(), 0);
    }
}
