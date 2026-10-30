use bevy::prelude::*;
use bevy_args::{Deserialize, Serialize, ValueEnum};

use crate::sort::SortMode;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize)]
pub enum DrawMode {
    #[default]
    All,
    Selected,
    HighlightSelected,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize, ValueEnum,
)]
pub enum GaussianMode {
    Gaussian2d,
    #[default]
    Gaussian3d,
    Gaussian4d,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize, ValueEnum,
)]
pub enum PlaybackMode {
    Loop,
    Once,
    Sin,
    #[default]
    Still,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize, ValueEnum,
)]
pub enum RasterizeMode {
    Classification,
    #[default]
    Color,
    Depth,
    Normal,
    OpticalFlow,
    Position,
    Velocity,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize, ValueEnum,
)]
pub enum RadixSortDepthBits {
    Bits16,
    Bits24,
    #[default]
    Bits32,
}

impl RadixSortDepthBits {
    pub const VARIANTS: [Self; 3] = [Self::Bits16, Self::Bits24, Self::Bits32];

    pub const fn bits(self) -> u32 {
        match self {
            Self::Bits16 => 16,
            Self::Bits24 => 24,
            Self::Bits32 => 32,
        }
    }

    pub const fn pipeline_index(self) -> usize {
        match self {
            Self::Bits16 => 0,
            Self::Bits24 => 1,
            Self::Bits32 => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize)]
pub enum GaussianColorSpace {
    #[default]
    SrgbRec709Display,
    LinRec709Display,
}

/// Generic footprint and alpha falloff selection. Existing assets retain legacy rendering.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize)]
pub enum GaussianRasterProfile {
    #[default]
    Legacy,
    NormalizedGaussian,
}

/// Transparency policy. Stochastic is experimental single-sample coverage,
/// not MCMC training or temporal accumulation. Use one policy across a view.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Reflect, Serialize, Deserialize)]
pub enum GaussianTransparency {
    #[default]
    Sorted,
    Stochastic,
}

impl GaussianTransparency {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "sorted" => Ok(Self::Sorted),
            "stochastic" => Ok(Self::Stochastic),
            _ => Err("transparency must be sorted or stochastic"),
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sorted => "sorted",
            Self::Stochastic => "stochastic",
        }
    }
}

// TODO: breakdown into components
#[derive(Component, Clone, Debug, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[serde(default)]
pub struct CloudSettings {
    pub aabb: bool,
    pub global_opacity: f32,
    pub global_scale: f32,
    pub opacity_adaptive_radius: bool,
    pub visualize_bounding_box: bool,
    pub sort_mode: SortMode,
    pub transparency: GaussianTransparency,
    /// Reproducible coverage seed, combined with cloud identity and record index.
    pub stochastic_seed: u32,
    /// Explicit sample index; fixed by default. No automatic accumulation occurs.
    pub stochastic_sample: u32,
    pub radix_sort_depth_bits: RadixSortDepthBits,
    pub draw_mode: DrawMode,
    pub gaussian_mode: GaussianMode,
    pub raster_profile: GaussianRasterProfile,
    pub playback_mode: PlaybackMode,
    pub rasterize_mode: RasterizeMode,
    pub color_space: GaussianColorSpace,
    /// Stably compact visible centers before radix sorting. Opt-in; other sort modes ignore it.
    #[serde(default)]
    pub visibility_compaction: bool,
    pub num_classes: usize,
    pub time: f32,
    pub time_scale: f32,
    pub time_start: f32,
    pub time_stop: f32,
    /// Additive/emissive blending. `false` (default) keeps premultiplied alpha-over, which is
    /// byte-identical to previous behaviour. When `true` the cloud composites `One + One`, so
    /// overlapping premultiplied fragments accumulate light and glow on a dark background instead
    /// of alpha-saturating into a solid blob. Additive draws have no occlusion (everything shows
    /// through), so it suits emissive/procedural content; leave it `false` for solid captures.
    pub additive: bool,
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            aabb: false,
            global_opacity: 1.0,
            global_scale: 1.0,
            opacity_adaptive_radius: true,
            visualize_bounding_box: false,
            sort_mode: SortMode::default(),
            transparency: GaussianTransparency::Sorted,
            stochastic_seed: 0,
            stochastic_sample: 0,
            radix_sort_depth_bits: RadixSortDepthBits::default(),
            draw_mode: DrawMode::default(),
            gaussian_mode: GaussianMode::default(),
            raster_profile: GaussianRasterProfile::default(),
            rasterize_mode: RasterizeMode::default(),
            color_space: GaussianColorSpace::default(),
            visibility_compaction: false,
            num_classes: 1,
            playback_mode: PlaybackMode::default(),
            time: 0.0,
            time_scale: 1.0,
            time_start: 0.0,
            time_stop: 1.0,
            additive: false,
        }
    }
}

impl CloudSettings {
    /// Unsupported inspection, additive and non-storage paths retain sorted rendering.
    pub fn stochastic(&self) -> bool {
        #[cfg(all(
            feature = "buffer_storage",
            feature = "sort_radix",
            not(feature = "webgl2")
        ))]
        {
            self.transparency == GaussianTransparency::Stochastic
                && self.sort_mode == SortMode::Radix
                && self.gaussian_mode == GaussianMode::Gaussian3d
                && self.rasterize_mode == RasterizeMode::Color
                && self.draw_mode == DrawMode::All
                && !self.additive
                && !self.visualize_bounding_box
        }
        #[cfg(not(all(
            feature = "buffer_storage",
            feature = "sort_radix",
            not(feature = "webgl2")
        )))]
        {
            false
        }
    }
}

#[derive(Default)]
pub struct SettingsPlugin;
impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CloudSettings>();
        app.register_type::<GaussianTransparency>();
        app.register_type::<RadixSortDepthBits>();
        app.register_type::<GaussianRasterProfile>();

        app.add_systems(Update, (playback_update,));
    }
}

fn playback_update(time: Res<Time>, mut query: Query<(&mut CloudSettings,)>) {
    for (mut settings,) in query.iter_mut() {
        if settings.time_scale == 0.0 {
            continue;
        }

        // bail condition
        match settings.playback_mode {
            PlaybackMode::Loop => {}
            PlaybackMode::Once => {
                if settings.time >= settings.time_stop {
                    continue;
                }
            }
            PlaybackMode::Sin => {}
            PlaybackMode::Still => {
                continue;
            }
        }

        // forward condition
        match settings.playback_mode {
            PlaybackMode::Loop | PlaybackMode::Once => {
                settings.time += time.delta_secs() * settings.time_scale;
            }
            PlaybackMode::Sin => {
                let theta = settings.time_scale * time.elapsed_secs();
                let y = (theta * 2.0 * std::f32::consts::PI).sin();
                settings.time = settings.time_start
                    + (settings.time_stop - settings.time_start) * (y + 1.0) / 2.0;
            }
            PlaybackMode::Still => {}
        }

        // reset condition
        match settings.playback_mode {
            PlaybackMode::Loop => {
                if settings.time > settings.time_stop {
                    settings.time = settings.time_start;
                }
            }
            PlaybackMode::Once => {}
            PlaybackMode::Sin => {}
            PlaybackMode::Still => {}
        }
    }
}

#[cfg(test)]
mod raster_profile_tests {
    use super::*;

    #[test]
    fn raster_profile_is_explicit_and_backwards_compatible() {
        let mut json = serde_json::to_value(CloudSettings::default()).unwrap();
        assert_eq!(
            json.get("raster_profile"),
            Some(&serde_json::json!("Legacy"))
        );
        json.as_object_mut().unwrap().remove("raster_profile");
        let restored = serde_json::from_value::<CloudSettings>(json).unwrap();
        assert_eq!(
            serde_json::to_value(restored).unwrap()["raster_profile"],
            "Legacy"
        );
    }
}
