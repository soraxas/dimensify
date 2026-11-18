//! Explicit renderer configuration; environment and query parsing belongs to consumers.
use crate::{renderer::SogRendererBackend, streaming::SogVisibilityPolicy};
use bevy::prelude::*;
use std::collections::HashMap;
/// Default for streams without `SogStreamRenderSettings`; read when a stream is
/// created. Experimental, and effective only with the Packed backend.
#[derive(Resource, Clone, Copy, Default)]
pub struct SogProjectionCache(pub bool);

/// Optional renderer configuration for a streamed scene. Set backend and
/// projection storage before its first update; changing either requires a
/// replacement stream entity. Compaction is read for new submissions; hosts
/// changing it must also update existing CloudSettings or submit a new selection.
/// The standalone viewer propagates compaction live on supported sorted GPU paths.
/// Streams without this component retain the application-wide defaults.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SogStreamRenderSettings {
    pub(crate) backend: SogRendererBackend,
    pub(crate) projection_cache: bool,
    pub compaction: bool,
}
impl SogStreamRenderSettings {
    /// Choose storage for a new stream. Replace the stream entity to change it.
    pub const fn new(backend: SogRendererBackend) -> Self {
        Self {
            backend,
            projection_cache: false,
            compaction: false,
        }
    }
    /// Configure projection storage before attaching to a new stream entity.
    pub const fn with_projection_cache(mut self, enabled: bool) -> Self {
        self.projection_cache = enabled;
        self
    }
    pub const fn backend(&self) -> SogRendererBackend {
        self.backend
    }
    pub const fn projection_cache(&self) -> bool {
        self.projection_cache
    }
}

/// Startup-only page residency experiment. Whole-file fallback remains default.
#[derive(Resource, Clone, Copy, Default)]
pub struct SogPagedSources(pub bool);

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SogRuntimeSettings {
    pub backend: SogRendererBackend,
    pub visibility: SogVisibilityPolicy,
    pub compaction: bool,
}

impl SogRuntimeSettings {
    /// Parse explicit renderer settings and report unsupported or invalid choices.
    pub fn parse(
        renderer: Option<&str>,
        visibility: Option<&str>,
        compaction: Option<&str>,
    ) -> (Self, Vec<String>) {
        let mut settings = Self::default();
        let mut warnings = Vec::new();
        for (name, value, valid) in [
            ("sog_renderer", renderer, &["packed", "snapshot", "gpu"][..]),
            ("sog_visibility", visibility, &["all", "frustum"][..]),
            ("sog_compaction", compaction, &["off", "on"][..]),
        ] {
            if let Some(value) = value {
                if !valid.contains(&value) {
                    warnings.push(format!("invalid {name}={value:?}; using {}", valid[0]));
                    continue;
                }
                match name {
                    "sog_renderer" if value == "snapshot" => {
                        settings.backend = SogRendererBackend::Snapshot
                    }
                    "sog_renderer" if value == "gpu" => {
                        settings.backend = SogRendererBackend::GpuResident
                    }
                    "sog_renderer" if value == "packed" => {
                        settings.backend = SogRendererBackend::Packed
                    }
                    "sog_visibility" if value == "frustum" => {
                        settings.visibility = SogVisibilityPolicy::Frustum
                    }
                    "sog_compaction" => settings.compaction = value == "on",
                    _ => {}
                }
            }
        }
        #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
        {
            if renderer.is_some_and(|value| value != "snapshot") {
                warnings.push("WebGL uses CPU snapshots; GPU SOG backends are unavailable".into());
            }
            settings.backend = SogRendererBackend::Snapshot;
            settings.compaction = false;
        }
        (settings, warnings)
    }
}

/// Logical last accepted selection for each live scene, including pending work.
#[derive(Resource, Default)]
pub struct SogSelectionStatistics {
    pub counts: HashMap<Entity, usize>,
    pub submitted_generations: u64,
    pub ready_generations: u64,
}

/// Stable diagnostic name for the selected source-residency backend.
pub fn backend_name(backend: SogRendererBackend) -> &'static str {
    match backend {
        SogRendererBackend::Snapshot => "snapshot",
        SogRendererBackend::GpuResident => "gpu",
        SogRendererBackend::Packed => "packed",
    }
}

/// Opt-in application-wide transparency policy. Install `apply_transparency`
/// after scene/stream updates and transform propagation to configure current
/// and pending clouds together.
/// Applications needing per-view policies should set CloudSettings directly.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SplatTransparency(pub TransparencyPolicy);

/// When clouds skip sorting, following SuperSplat's `view.stochastic` modes.
/// Stochastic coverage avoids the per-frame sort at the cost of visible grain;
/// the adaptive modes use it only while the eye is least able to see that grain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransparencyPolicy {
    /// Always sort and blend.
    Sorted,
    /// Always draw stochastic coverage (experimental).
    Stochastic,
    /// Stochastic while a Gaussian camera moves, sorted once it has settled.
    Movement,
    /// `Movement` while recent sorted frames are slower than
    /// [`AUTO_ENGAGE_MS`], otherwise `Sorted`.
    #[default]
    Auto,
}

impl TransparencyPolicy {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "sorted" => Ok(Self::Sorted),
            "stochastic" => Ok(Self::Stochastic),
            "movement" => Ok(Self::Movement),
            "auto" => Ok(Self::Auto),
            _ => Err("transparency must be sorted, stochastic, movement or auto"),
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sorted => "sorted",
            Self::Stochastic => "stochastic",
            Self::Movement => "movement",
            Self::Auto => "auto",
        }
    }
    /// Whether settled frames sort; only `Stochastic` never does.
    pub const fn sorts(self) -> bool {
        !matches!(self, Self::Stochastic)
    }
    const fn adaptive(self) -> bool {
        matches!(self, Self::Movement | Self::Auto)
    }
}

impl From<bevy_gaussian_splatting::GaussianTransparency> for TransparencyPolicy {
    fn from(mode: bevy_gaussian_splatting::GaussianTransparency) -> Self {
        match mode {
            bevy_gaussian_splatting::GaussianTransparency::Sorted => Self::Sorted,
            bevy_gaussian_splatting::GaussianTransparency::Stochastic => Self::Stochastic,
        }
    }
}

/// Camera stillness before adaptive modes return to sorted. Touch and damped
/// navigation leave unchanged frames mid-drag; alternating modes there would
/// add the slow sorted frames back into the motion.
pub const SETTLE_SECONDS: f32 = 0.15;
/// `Auto` engages once the median recent sorted frame misses 40 fps. This is
/// whole-frame time, so it also works without GPU timestamp queries.
pub const AUTO_ENGAGE_MS: f32 = 25.0;
const AUTO_SAMPLES: usize = 30;
const AUTO_MIN_SAMPLES: usize = 5;

/// Last resolution of [`SplatTransparency`], for display and telemetry.
#[derive(Resource, Clone, Debug, Default)]
pub struct SplatTransparencyState {
    /// Mode applied to clouds this frame.
    pub effective: bevy_gaussian_splatting::GaussianTransparency,
    pub moving: bool,
    /// Whether `Auto` currently treats sorted frames as slow.
    pub engaged: bool,
    /// Median of recent sorted frame intervals.
    pub sorted_frame_ms: Option<f32>,
}

#[derive(Default)]
pub struct TransparencyTracker {
    views: Vec<(Entity, GlobalTransform)>,
    still_for: Option<f32>,
    samples: std::collections::VecDeque<f32>,
    engaged: bool,
    previous: Option<bevy_gaussian_splatting::GaussianTransparency>,
}

impl TransparencyTracker {
    fn observe(&mut self, views: impl Iterator<Item = (Entity, GlobalTransform)>, delta: f32) {
        let views: Vec<_> = views.collect();
        // A new or removed camera is not motion; only a changed pose is.
        let moved = views.iter().any(|(entity, transform)| {
            self.views
                .iter()
                .any(|(seen, previous)| seen == entity && previous != transform)
        });
        self.views = views;
        self.still_for = Some(if moved {
            0.
        } else {
            self.still_for.map_or(f32::INFINITY, |still| still + delta)
        });
        // This frame's interval measures the frame drawn with the previous mode.
        if self.previous == Some(bevy_gaussian_splatting::GaussianTransparency::Sorted)
            && delta > 0.
        {
            if self.samples.len() == AUTO_SAMPLES {
                self.samples.pop_front();
            }
            self.samples.push_back(delta * 1000.);
            if let Some(median) = self.median() {
                self.engaged = median > AUTO_ENGAGE_MS;
            }
        }
    }
    fn median(&self) -> Option<f32> {
        if self.samples.len() < AUTO_MIN_SAMPLES {
            return None;
        }
        let mut sorted: Vec<_> = self.samples.iter().copied().collect();
        sorted.sort_by(f32::total_cmp);
        Some(sorted[sorted.len() / 2])
    }
    fn moving(&self) -> bool {
        self.still_for.is_some_and(|still| still < SETTLE_SECONDS)
    }
    fn resolve(&self, policy: TransparencyPolicy) -> bevy_gaussian_splatting::GaussianTransparency {
        use bevy_gaussian_splatting::GaussianTransparency::{Sorted, Stochastic};
        let stochastic = match policy {
            TransparencyPolicy::Sorted => false,
            TransparencyPolicy::Stochastic => true,
            TransparencyPolicy::Movement => self.moving(),
            TransparencyPolicy::Auto => self.moving() && self.engaged,
        };
        if stochastic { Stochastic } else { Sorted }
    }
}

pub fn apply_transparency(
    mode: Res<SplatTransparency>,
    time: Res<Time<Real>>,
    cameras: Query<(Entity, &GlobalTransform), With<bevy_gaussian_splatting::GaussianCamera>>,
    state: Option<ResMut<SplatTransparencyState>>,
    mut tracker: Local<TransparencyTracker>,
    mut clouds: Query<&mut bevy_gaussian_splatting::CloudSettings>,
) {
    use bevy_gaussian_splatting::GaussianTransparency::Sorted;
    tracker.observe(
        cameras
            .iter()
            .map(|(entity, transform)| (entity, *transform)),
        time.delta_secs(),
    );
    let effective = tracker.resolve(mode.0);
    let warmup = mode.0.adaptive();
    let mut drawn = Sorted;
    for mut settings in &mut clouds {
        // Adaptive modes leave paths without stochastic support (WebGL, CPU
        // sorting) sorted, so a CPU-built cloud is not rebuilt on every switch.
        let target = if warmup && !settings.stochastic_supported() {
            Sorted
        } else {
            effective
        };
        if settings.transparency != target {
            settings.transparency = target;
        }
        if settings.transparency_warmup != warmup {
            settings.transparency_warmup = warmup;
        }
        if settings.stochastic() {
            drawn = target;
        }
    }
    tracker.previous = Some(drawn);
    if let Some(mut state) = state {
        *state = SplatTransparencyState {
            effective: drawn,
            moving: tracker.moving(),
            engaged: tracker.engaged,
            sorted_frame_ms: tracker.median(),
        };
    }
}

#[cfg(test)]
mod transparency_tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use bevy_gaussian_splatting::{CloudSettings, GaussianCamera};
    use std::time::Duration;

    fn app(policy: TransparencyPolicy, frame_ms: u64) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                frame_ms,
            )))
            .insert_resource(SplatTransparency(policy))
            .init_resource::<SplatTransparencyState>()
            .add_systems(Update, apply_transparency);
        app
    }

    fn stochastic(app: &App, entity: Entity) -> bool {
        app.world()
            .get::<CloudSettings>(entity)
            .unwrap()
            .stochastic()
    }

    fn turn(app: &mut App, camera: Entity) {
        let angle = app.world().resource::<Time<Real>>().elapsed_secs();
        app.world_mut()
            .get_mut::<GlobalTransform>(camera)
            .unwrap()
            .set_if_neq(GlobalTransform::from_rotation(Quat::from_rotation_y(angle)));
    }

    #[test]
    fn policy_updates_existing_and_new_pending_clouds_without_changing_appearance() {
        let mut app = app(TransparencyPolicy::Stochastic, 16);
        let old = app
            .world_mut()
            .spawn(CloudSettings {
                global_opacity: 0.4,
                stochastic_seed: 123,
                ..default()
            })
            .id();
        app.update();
        let pending = app.world_mut().spawn(CloudSettings::default()).id();
        app.update();
        for entity in [old, pending] {
            assert!(stochastic(&app, entity));
        }
        app.world_mut().resource_mut::<SplatTransparency>().0 = TransparencyPolicy::Sorted;
        app.update();
        for entity in [old, pending] {
            assert!(!stochastic(&app, entity));
        }
        let preserved = app.world().get::<CloudSettings>(old).unwrap();
        assert_eq!(preserved.global_opacity, 0.4);
        assert_eq!(preserved.stochastic_seed, 123);
        assert!(
            !preserved.transparency_warmup,
            "fixed policies compile one variant"
        );
    }

    #[test]
    fn movement_skips_sorting_only_until_the_camera_settles() {
        let mut app = app(TransparencyPolicy::Movement, 16);
        let camera = app
            .world_mut()
            .spawn((GaussianCamera::default(), GlobalTransform::default()))
            .id();
        let cloud = app.world_mut().spawn(CloudSettings::default()).id();
        app.update();
        assert!(!stochastic(&app, cloud), "a new camera is not motion");
        assert!(
            app.world()
                .get::<CloudSettings>(cloud)
                .unwrap()
                .transparency_warmup
        );
        for _ in 0..5 {
            app.update();
            turn(&mut app, camera);
        }
        app.update();
        assert!(stochastic(&app, cloud));
        // Unchanged frames inside the settle window keep the drag stochastic.
        for _ in 0..(SETTLE_SECONDS * 1000. / 16.) as usize - 1 {
            app.update();
            assert!(stochastic(&app, cloud));
        }
        for _ in 0..2 {
            app.update();
        }
        assert!(!stochastic(&app, cloud));
        let state = app.world().resource::<SplatTransparencyState>();
        assert!(!state.moving);
    }

    #[test]
    fn auto_engages_only_when_sorted_frames_are_slow() {
        for (frame_ms, engaged) in [(16, false), (40, true)] {
            let mut app = app(TransparencyPolicy::Auto, frame_ms);
            let camera = app
                .world_mut()
                .spawn((GaussianCamera::default(), GlobalTransform::default()))
                .id();
            let cloud = app.world_mut().spawn(CloudSettings::default()).id();
            for _ in 0..AUTO_MIN_SAMPLES + 2 {
                app.update();
                assert!(!stochastic(&app, cloud), "still frames always sort");
            }
            turn(&mut app, camera);
            app.update();
            assert_eq!(stochastic(&app, cloud), engaged, "{frame_ms} ms frames");
            let state = app.world().resource::<SplatTransparencyState>();
            assert_eq!(state.engaged, engaged);
            assert_eq!(state.sorted_frame_ms, Some(frame_ms as f32));
        }
    }

    #[test]
    fn auto_ignores_stochastic_frames_and_disengages_on_fast_sorted_frames() {
        let mut app = app(TransparencyPolicy::Auto, 40);
        let camera = app
            .world_mut()
            .spawn((GaussianCamera::default(), GlobalTransform::default()))
            .id();
        let cloud = app.world_mut().spawn(CloudSettings::default()).id();
        for _ in 0..AUTO_MIN_SAMPLES + 2 {
            app.update();
        }
        // Fast stochastic frames say nothing about the cost of sorting.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(8)));
        for _ in 0..AUTO_SAMPLES * 2 {
            turn(&mut app, camera);
            app.update();
        }
        assert!(stochastic(&app, cloud));
        assert!(app.world().resource::<SplatTransparencyState>().engaged);
        // A lighter view: settled sorted frames are now fast.
        for _ in 0..AUTO_SAMPLES {
            app.update();
        }
        turn(&mut app, camera);
        app.update();
        assert!(!stochastic(&app, cloud));
        assert!(!app.world().resource::<SplatTransparencyState>().engaged);
    }

    #[test]
    fn unsupported_clouds_stay_sorted_without_churning_settings() {
        let mut app = app(TransparencyPolicy::Movement, 16);
        let camera = app
            .world_mut()
            .spawn((GaussianCamera::default(), GlobalTransform::default()))
            .id();
        app.update();
        turn(&mut app, camera);
        let cloud = app
            .world_mut()
            .spawn(CloudSettings {
                sort_mode: bevy_gaussian_splatting::sort::SortMode::Std,
                ..default()
            })
            .id();
        app.update();
        let settings = app.world().get::<CloudSettings>(cloud).unwrap();
        assert_eq!(
            settings.transparency,
            bevy_gaussian_splatting::GaussianTransparency::Sorted
        );
        assert_eq!(
            app.world().resource::<SplatTransparencyState>().effective,
            bevy_gaussian_splatting::GaussianTransparency::Sorted
        );
    }

    #[test]
    fn policy_names_round_trip() {
        for policy in [
            TransparencyPolicy::Sorted,
            TransparencyPolicy::Stochastic,
            TransparencyPolicy::Movement,
            TransparencyPolicy::Auto,
        ] {
            assert_eq!(TransparencyPolicy::parse(policy.name()), Ok(policy));
        }
        assert!(TransparencyPolicy::parse("dither").is_err());
    }
}
