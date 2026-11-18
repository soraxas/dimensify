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
pub const EFFECT_MAGIC: u32 = 4;
pub const EFFECT_SPREAD: u32 = 8;
pub const EFFECT_RIPPLE: u32 = 16;
/// Modifies [`EFFECT_MAGIC`]: open with the azimuth wipe before the front.
pub const EFFECT_MAGIC_WIPE: u32 = 32;
pub const EFFECT_STARS: u32 = 64;
/// Concurrent click ripples; matches the array in `GaussianUniforms`.
pub const RIPPLE_SLOTS: usize = 4;

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
    /// Spark's Magic: a wipe around the vertical axis conjures drifting
    /// sparkles, then a glowing front settles them into splats.
    pub magic: bool,
    /// Open Magic with Spark's azimuth wipe; without it the glowing front
    /// grows from `origin` straight away.
    pub magic_wipe: bool,
    /// Spark's Spread: the scene relaxes outward from `origin` while radial
    /// waves grow grey dots, flood in colour, then grow full splats.
    pub spread: bool,
    /// After World Labs' homepage: a sphere grows from `origin`, cutting
    /// pulsing coloured points into full splats behind a tinted edge.
    pub stars: bool,
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
    pub magic_duration: f32,
    pub spread_duration: f32,
    pub stars_duration: f32,
    /// Click ripples: xyz world origin, w radius. Ripples are feedback, not
    /// load stages: they play over any load effect and never extend it.
    pub ripples: [Vec4; RIPPLE_SLOTS],
    /// Seconds since each ripple started; negative marks an empty slot.
    pub ripple_ages: Vec4,
    pub ripple_duration: f32,
    /// Linear glow colour; alpha is intensity.
    pub glow: Vec4,
}

impl Default for SplatEffects {
    fn default() -> Self {
        Self {
            enabled: false,
            reveal: false,
            explosion: false,
            magic: false,
            magic_wipe: false,
            spread: false,
            stars: false,
            time: 0.0,
            origin: Vec3::ZERO,
            extent: 1.0,
            axes: Vec3::new(1.0, 0.0, 1.0),
            near: 0.0,
            reveal_duration: 3.0,
            reveal_edge: 0.08,
            explosion_duration: 2.0,
            magic_duration: 5.0,
            spread_duration: 5.0,
            stars_duration: 4.5,
            ripples: [Vec4::ZERO; RIPPLE_SLOTS],
            ripple_ages: Vec4::splat(-1.0),
            ripple_duration: 1.2,
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
        if self.magic && self.time < self.magic_duration {
            mask |= EFFECT_MAGIC;
            if self.magic_wipe {
                mask |= EFFECT_MAGIC_WIPE;
            }
        }
        if self.spread && self.time < self.spread_duration {
            mask |= EFFECT_SPREAD;
        }
        if self.stars && self.time < self.stars_duration {
            mask |= EFFECT_STARS;
        }
        if self
            .ripple_ages
            .to_array()
            .iter()
            .any(|&age| (0.0..self.ripple_duration).contains(&age))
        {
            mask |= EFFECT_RIPPLE;
        }
        mask
    }

    /// Whether the plain pipeline may stand in while the effect variant
    /// compiles: no playing stage hides splats, so skipping a ripple's first
    /// frames is better than a blank scene.
    pub fn plain_fallback(&self) -> bool {
        self.mask() & !EFFECT_RIPPLE == 0
    }

    /// Longest duration among the selected stages.
    pub fn duration(&self) -> f32 {
        [
            (self.reveal, self.reveal_duration),
            (self.explosion, self.explosion_duration),
            (self.magic, self.magic_duration),
            (self.spread, self.spread_duration),
            (self.stars, self.stars_duration),
        ]
        .into_iter()
        .filter(|&(selected, _)| selected)
        .fold(0.0, |longest, (_, duration)| duration.max(longest))
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
            magic: true,
            spread: true,
            reveal_duration: 3.0,
            explosion_duration: 2.0,
            magic_duration: 4.0,
            spread_duration: 2.5,
            ..default()
        };
        assert_eq!(
            effects.mask(),
            EFFECT_REVEAL | EFFECT_EXPLOSION | EFFECT_MAGIC | EFFECT_SPREAD
        );
        effects.time = 2.5;
        assert_eq!(effects.mask(), EFFECT_REVEAL | EFFECT_MAGIC);
        effects.time = 3.0;
        assert_eq!(effects.mask(), EFFECT_MAGIC);
        effects.time = 4.0;
        assert!(effects.finished());
        assert_eq!(effects.duration(), 4.0);
    }

    #[test]
    fn ripples_play_over_any_load_stage_without_extending_it() {
        let mut effects = SplatEffects {
            enabled: true,
            ripple_duration: 1.0,
            ..default()
        };
        assert_eq!(effects.mask(), 0);
        effects.ripple_ages.y = 0.5;
        assert_eq!(effects.mask(), EFFECT_RIPPLE);
        assert!(effects.plain_fallback());
        assert_eq!(effects.duration(), 0.0);
        effects.reveal = true;
        assert_eq!(effects.mask(), EFFECT_REVEAL | EFFECT_RIPPLE);
        assert!(!effects.plain_fallback());
        effects.ripple_ages.y = 1.0;
        effects.reveal = false;
        assert!(effects.finished());
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
