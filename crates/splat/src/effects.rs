//! Opt-in load animations, played once when each scene first becomes visible.
//!
//! The renderer evaluates the stages per splat (`render/effects.wesl` in the
//! vendored renderer). This module owns the per-scene clock and bounds: clouds
//! are replaced as streamed generations hand off, so the clock, origin and
//! extent belong to the scene root and are stamped onto every current cloud.
use bevy::{camera::primitives::Aabb, prelude::*};
use bevy_gaussian_splatting::{
    CloudSettings, GaussianCamera, SplatEffects, gaussian::effects::RIPPLE_SLOTS,
};
use std::collections::HashMap;

/// Composable load animation stages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadEffect {
    #[default]
    None,
    /// A glowing front sweeps outward over the scene.
    Reveal,
    /// Splats burst out of the scene centre into place.
    Explosion,
    /// Both stages at once.
    RevealExplosion,
    /// Spark's Magic: a wipe conjures drifting sparkles, then a glowing front
    /// settles them into splats.
    Magic,
    /// Magic's glowing front alone, growing outward from the origin.
    MagicFront,
    /// Spark's Spread: the scene relaxes outward as waves of grey dots gain
    /// colour and size.
    Spread,
    /// After World Labs' homepage: a sphere cuts pulsing coloured points into
    /// full splats.
    Stars,
}

impl LoadEffect {
    pub const ALL: [Self; 8] = [
        Self::None,
        Self::Reveal,
        Self::Explosion,
        Self::RevealExplosion,
        Self::Magic,
        Self::MagicFront,
        Self::Spread,
        Self::Stars,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Reveal => "reveal",
            Self::Explosion => "explosion",
            Self::RevealExplosion => "reveal-explosion",
            Self::Magic => "magic",
            Self::MagicFront => "magic-front",
            Self::Spread => "spread",
            Self::Stars => "stars",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        Self::ALL
            .into_iter()
            .find(|effect| effect.name() == value)
            .ok_or("effect must be none, reveal, explosion, reveal-explosion, magic, magic-front, spread or stars")
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|&e| e == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// Spark's effects are ground-plane reveals: they measure distance across
    /// the ground around any origin instead of growing a sphere from it.
    fn ground(self) -> bool {
        matches!(self, Self::Magic | Self::MagicFront | Self::Spread)
    }

    /// Selected stages, with the pipeline enabled unless none are.
    fn stages(self) -> SplatEffects {
        let stages = match self {
            Self::None => SplatEffects::default(),
            Self::Reveal => SplatEffects {
                reveal: true,
                ..default()
            },
            Self::Explosion => SplatEffects {
                explosion: true,
                ..default()
            },
            Self::RevealExplosion => SplatEffects {
                reveal: true,
                explosion: true,
                ..default()
            },
            Self::Magic => SplatEffects {
                magic: true,
                magic_wipe: true,
                ..default()
            },
            Self::MagicFront => SplatEffects {
                magic: true,
                ..default()
            },
            Self::Spread => SplatEffects {
                spread: true,
                ..default()
            },
            Self::Stars => SplatEffects {
                stars: true,
                ..default()
            },
        };
        SplatEffects {
            enabled: self != Self::None,
            ..stages
        }
    }
}

/// Where the load effect radiates from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadEffectOrigin {
    /// The surface the camera looks at when the effect starts: a host pick
    /// through [`LoadEffectTarget`], else where the view ray crosses the scene
    /// bounds. The reveal grows as a sphere from that point.
    #[default]
    Target,
    /// The Gaussian camera position when the effect starts. The reveal grows
    /// as a sphere around the viewer; the explosion streams out of the eye.
    Camera,
    /// The centre of the scene bounds. The reveal sweeps a disc across the
    /// ground plane; the explosion bursts from the middle of the scene.
    Center,
}

impl LoadEffectOrigin {
    pub fn name(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Camera => "camera",
            Self::Center => "center",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "target" => Ok(Self::Target),
            "camera" => Ok(Self::Camera),
            "center" => Ok(Self::Center),
            _ => Err("effect origin must be target, camera or center"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Target => Self::Camera,
            Self::Camera => Self::Center,
            Self::Center => Self::Target,
        }
    }

    fn axes(self) -> Vec3 {
        match self {
            Self::Target | Self::Camera => Vec3::ONE,
            Self::Center => Vec3::new(1., 0., 1.),
        }
    }
}

/// Opt-in application-wide load effect. Install `apply_load_effect` after
/// scene/stream updates and transform propagation, like `apply_transparency`.
#[derive(Resource, Clone, Copy, Debug)]
pub struct SplatLoadEffect {
    pub effect: LoadEffect,
    pub origin: LoadEffectOrigin,
    /// Playback rate: 2 plays every stage in half its duration, 0.5 in double.
    pub speed: f32,
    /// Increment to restart every scene's animation (recapturing the camera).
    pub replay: u32,
}

impl Default for SplatLoadEffect {
    fn default() -> Self {
        Self {
            effect: LoadEffect::default(),
            origin: LoadEffectOrigin::default(),
            speed: 1.,
            replay: 0,
        }
    }
}

/// Playback rates offered by the viewer's settings.
pub const EFFECT_SPEEDS: [f32; 5] = [0.5, 0.75, 1., 1.5, 2.];

/// The next offered rate after `speed`, wrapping to the slowest.
pub fn next_effect_speed(speed: f32) -> f32 {
    EFFECT_SPEEDS
        .into_iter()
        .find(|&s| s > speed + 1e-3)
        .unwrap_or(EFFECT_SPEEDS[0])
}

pub fn parse_effect_speed(value: &str) -> Result<f32, &'static str> {
    value
        .parse::<f32>()
        .ok()
        .filter(|s| (0.1..=10.).contains(s))
        .ok_or("effect speed must be a number from 0.1 to 10")
}

/// Stretch every load stage for a playback `speed`; ripples keep their pace.
fn at_speed(mut effects: SplatEffects, speed: f32) -> SplatEffects {
    let stretch = 1. / speed.clamp(0.1, 10.);
    for duration in [
        &mut effects.reveal_duration,
        &mut effects.explosion_duration,
        &mut effects.magic_duration,
        &mut effects.spread_duration,
        &mut effects.stars_duration,
    ] {
        *duration *= stretch;
    }
    effects
}

/// The effect clock advances at most this much per frame, so a frame hitch
/// (for example a streamed generation's upload) slows the animation for a
/// moment instead of making it jump ahead.
const MAX_CLOCK_STEP_SECS: f32 = 1. / 30.;

/// Look-at handshake for [`LoadEffectOrigin::Target`]. `apply_load_effect`
/// sets `wanted` while a scene waits for its origin; a host with a surface pick
/// (the viewer's depth pick at the view centre) writes `answer`: `Some(hit)`
/// or `None` for a miss. Without an answer the bounds estimate is used.
#[derive(Resource, Default, Debug)]
pub struct LoadEffectTarget {
    pub wanted: bool,
    pub answer: Option<Option<Vec3>>,
}

/// Feedback ripples on the scene surface, such as where the user marks a
/// point to annotate. `apply_load_effect` ages them and stamps them onto every
/// cloud, over whatever load effect is playing.
#[derive(Resource, Debug, Default)]
pub struct SplatRipples {
    /// World origin, radius and age in seconds, oldest first.
    active: Vec<(Vec3, f32, f32)>,
    armed: bool,
}

impl SplatRipples {
    /// Start a ripple at `origin` reaching `radius` world units, replacing
    /// the oldest when every slot is busy.
    pub fn spawn(&mut self, origin: Vec3, radius: f32) {
        if self.active.len() == RIPPLE_SLOTS {
            self.active.remove(0);
        }
        self.active.push((origin, radius, 0.));
        self.armed = false;
    }

    /// Compile the effect stage ahead of a likely ripple (for example on
    /// button press, while the surface pick resolves). The plain cloud keeps
    /// drawing meanwhile.
    pub fn arm(&mut self, armed: bool) {
        self.armed = armed;
    }

    fn advance(&mut self, delta: f32, duration: f32) {
        for (_, _, age) in &mut self.active {
            *age += delta;
        }
        self.active.retain(|&(_, _, age)| age < duration);
    }

    fn stamp(&self, effects: &mut SplatEffects) {
        effects.enabled |= self.armed || !self.active.is_empty();
        let mut ages = [-1.; RIPPLE_SLOTS];
        for (slot, &(origin, radius, age)) in self.active.iter().enumerate() {
            effects.ripples[slot] = origin.extend(radius);
            ages[slot] = age;
        }
        effects.ripple_ages = Vec4::from_array(ages);
    }
}

/// A scene that appears waits until the camera has held still this long, so
/// the effect neither starts on a streamed scene's coarse first generation nor
/// plays behind the viewer's animated frame-on-load, which begins only after
/// the scene appears.
const CAMERA_STILL_SECS: f32 = 0.25;
/// The wait for a still camera gives up after this long.
const CAMERA_SETTLE_SECS: f32 = 2.0;
/// Further time a target origin waits for a host pick before estimating.
const TARGET_PICK_SECS: f32 = 0.5;

/// Where the view ray first meets the scene: the point along the ray closest
/// to the bounds centre, kept within the ray's span inside the bounds.
fn view_target(eye: Vec3, forward: Vec3, low: Vec3, high: Vec3) -> Vec3 {
    let inverse = forward.recip();
    let a = (low - eye) * inverse;
    let b = (high - eye) * inverse;
    let enter = a.min(b).max_element().max(0.);
    let exit = a.max(b).min_element();
    let center = (low + high) * 0.5;
    if enter.is_nan() || exit.is_nan() || enter > exit {
        return center;
    }
    eye + forward * (center - eye).dot(forward).clamp(enter, exit)
}

/// A viewed effect reaches at most this many look distances past its near point.
const EXTENT_LOOKS: f32 = 4.;

/// Floaters stretch scene bounds far past what the camera frames, and the
/// front would spend the animation crossing them. Cap the extent at
/// [`EXTENT_LOOKS`] look distances past `near`, but never below a fortieth
/// of the bounds' span per look, so a pick right at the eye still animates.
fn capped_extent(extent: f32, near: f32, look: f32) -> f32 {
    let span = extent - near;
    extent.min(near + EXTENT_LOOKS * look.max(0.025 * span))
}

struct Playback {
    time: f32,
    origin: Vec3,
    extent: f32,
    near: f32,
    axes: Vec3,
}

#[derive(Default)]
pub struct LoadEffectState {
    scenes: HashMap<Entity, Playback>,
    /// Seconds each visible scene has waited, and how long the camera has
    /// held still within that wait.
    waiting: HashMap<Entity, (f32, f32)>,
    camera: Option<Vec3>,
    replay: u32,
}

/// World-space bounds of a local `Aabb` under `transform`.
fn world_bounds(aabb: &Aabb, transform: &GlobalTransform) -> (Vec3, Vec3) {
    let affine = transform.affine();
    let center = affine.transform_point3a(aabb.center);
    let half = affine.matrix3.abs() * aabb.half_extents;
    ((center - half).into(), (center + half).into())
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn apply_load_effect(
    config: Res<SplatLoadEffect>,
    time: Res<Time>,
    mut state: Local<LoadEffectState>,
    parents: Query<&ChildOf>,
    cameras: Query<&GlobalTransform, With<GaussianCamera>>,
    mut target: Option<ResMut<LoadEffectTarget>>,
    mut ripples: Option<ResMut<SplatRipples>>,
    sources: Query<(&crate::scene::SourceGeometry, &Transform)>,
    mut clouds: Query<(
        Entity,
        &mut CloudSettings,
        Option<&Aabb>,
        &GlobalTransform,
        Option<&InheritedVisibility>,
    )>,
) {
    if state.replay != config.replay {
        state.scenes.clear();
        state.waiting.clear();
        state.replay = config.replay;
    }
    let view = cameras
        .iter()
        .next()
        .map(|t| (t.translation(), t.forward().as_vec3()));
    let camera = view.map(|(eye, _)| eye);
    let camera_moved = match (camera, state.camera) {
        (Some(now), Some(before)) => now.distance_squared(before) > 1e-8,
        _ => true,
    };
    state.camera = camera;
    let template = at_speed(config.effect.stages(), config.speed);

    // Bounds of the visible clouds under each scene root.
    let mut visible: HashMap<Entity, (Vec3, Vec3)> = HashMap::new();
    let mut live = Vec::new();
    // Streamed clouds hang below their stream entity; whole-file clouds are roots.
    let scene_of = |mut entity: Entity| {
        while let Ok(parent) = parents.get(entity) {
            entity = parent.parent();
        }
        entity
    };
    for (entity, _, aabb, transform, shown) in &clouds {
        let root = scene_of(entity);
        live.push(root);
        if !shown.is_some_and(|v| v.get()) {
            continue;
        }
        let (low, high) = aabb.map_or((transform.translation(), transform.translation()), |a| {
            world_bounds(a, transform)
        });
        visible
            .entry(root)
            .and_modify(|(l, h)| {
                *l = l.min(low);
                *h = h.max(high);
            })
            .or_insert((low, high));
    }
    // A loaded scene's source bounds exclude what its clouds' boxes may carry,
    // such as a streamed scene's environment splats far out around it.
    for (root, bounds) in &mut visible {
        if let Ok((source, transform)) = sources.get(*root) {
            *bounds = source.transformed_bounds(*transform);
        }
    }
    state.scenes.retain(|root, _| live.contains(root));
    state.waiting.retain(|root, _| live.contains(root));
    let delta = time.delta_secs();
    if let Some(ripples) = ripples.as_mut() {
        ripples.advance(delta, template.ripple_duration);
    }
    let duration = template.duration();
    for playback in state.scenes.values_mut() {
        playback.time = (playback.time + delta.min(MAX_CLOCK_STEP_SECS)).min(duration);
    }
    let mut want_target = false;
    if template.enabled {
        for (root, (low, high)) in visible {
            if state.scenes.contains_key(&root) {
                continue;
            }
            // Hold the first frame until the camera has settled on the scene.
            let waited = if view.is_some() {
                let (waited, still) = state.waiting.entry(root).or_default();
                *waited += delta;
                *still = if camera_moved { 0. } else { *still + delta };
                if *still < CAMERA_STILL_SECS && *waited < CAMERA_SETTLE_SECS {
                    continue;
                }
                *waited
            } else {
                0.
            };
            let (origin, mode) = match (config.origin, view) {
                (LoadEffectOrigin::Center, _) | (_, None) => {
                    ((low + high) * 0.5, LoadEffectOrigin::Center)
                }
                (mode, Some((eye, forward))) => {
                    if mode == LoadEffectOrigin::Camera {
                        (eye, mode)
                    } else {
                        let answer = target.as_ref().and_then(|t| t.answer);
                        let host = target.is_some();
                        match answer {
                            Some(Some(hit)) => (hit, mode),
                            None if host && waited < CAMERA_SETTLE_SECS + TARGET_PICK_SECS => {
                                want_target = true;
                                continue;
                            }
                            _ => (view_target(eye, forward, low, high), mode),
                        }
                    }
                }
            };
            state.waiting.remove(&root);
            let axes = if config.effect.ground() {
                LoadEffectOrigin::Center.axes()
            } else {
                mode.axes()
            };
            let extent = (0..8)
                .map(|i| {
                    let corner = Vec3::new(
                        if i & 1 == 0 { low.x } else { high.x },
                        if i & 2 == 0 { low.y } else { high.y },
                        if i & 4 == 0 { low.z } else { high.z },
                    );
                    ((corner - origin) * axes).length()
                })
                .fold(1e-3, f32::max);
            // Zero when the origin is inside the bounds.
            let near = ((low - origin).max(origin - high).max(Vec3::ZERO) * axes).length();
            let extent = match view {
                Some((eye, forward)) if mode != LoadEffectOrigin::Center => {
                    let look = if mode == LoadEffectOrigin::Camera {
                        view_target(eye, forward, low, high)
                    } else {
                        origin
                    };
                    capped_extent(extent, near, eye.distance(look))
                }
                _ => extent,
            };
            state.scenes.insert(
                root,
                Playback {
                    time: 0.,
                    origin,
                    extent,
                    near,
                    axes,
                },
            );
        }
    }

    if let Some(target) = target.as_mut() {
        // A fresh request starts without a stale answer.
        if !want_target {
            if target.wanted || target.answer.is_some() {
                target.wanted = false;
                target.answer = None;
            }
        } else if !target.wanted {
            target.wanted = true;
        }
    }

    for (entity, mut settings, ..) in &mut clouds {
        let root = scene_of(entity);
        let mut effects = match state.scenes.get(&root) {
            Some(playback) if template.enabled => SplatEffects {
                time: playback.time,
                origin: playback.origin,
                extent: playback.extent,
                axes: playback.axes,
                near: playback.near,
                ..template
            },
            // Not yet visible: hold the first frame so nothing pops in early.
            _ => template,
        };
        if let Some(ripples) = ripples.as_ref() {
            ripples.stamp(&mut effects);
        }
        if settings.effects != effects {
            settings.effects = effects;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use bevy_gaussian_splatting::gaussian::effects::EFFECT_RIPPLE;
    use std::time::Duration;

    fn app(effect: LoadEffect) -> App {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                500,
            )))
            .insert_resource(SplatLoadEffect {
                effect,
                origin: LoadEffectOrigin::Center,
                ..default()
            })
            .add_systems(Update, apply_load_effect);
        app
    }

    fn cloud(app: &mut App, visible: bool, translation: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                CloudSettings::default(),
                Aabb::from_min_max(Vec3::splat(-1.), Vec3::new(1., 1., 3.)),
                GlobalTransform::from_translation(translation),
                if visible {
                    InheritedVisibility::VISIBLE
                } else {
                    InheritedVisibility::HIDDEN
                },
            ))
            .id()
    }

    fn effects(app: &App, entity: Entity) -> SplatEffects {
        app.world().get::<CloudSettings>(entity).unwrap().effects
    }

    #[test]
    fn effect_names_round_trip() {
        for effect in LoadEffect::ALL {
            assert_eq!(LoadEffect::parse(effect.name()), Ok(effect));
        }
        assert!(LoadEffect::parse("sparkle").is_err());
        assert_eq!(LoadEffect::Stars.next(), LoadEffect::None);
    }

    #[test]
    fn none_leaves_the_pipeline_unspecialized() {
        let mut app = app(LoadEffect::None);
        let entity = cloud(&mut app, true, Vec3::ZERO);
        app.update();
        assert_eq!(effects(&app, entity), SplatEffects::default());
    }

    #[test]
    fn clock_starts_when_visible_and_stops_at_the_duration() {
        let mut app = app(LoadEffect::Reveal);
        let entity = cloud(&mut app, false, Vec3::new(10., 0., 0.));
        app.update();
        app.update();
        let held = effects(&app, entity);
        assert!(held.enabled && held.time == 0. && held.mask() != 0);

        app.world_mut()
            .entity_mut(entity)
            .insert(InheritedVisibility::VISIBLE);
        app.update();
        let started = effects(&app, entity);
        assert_eq!(started.origin, Vec3::new(10., 0., 1.));
        // Farthest ground-plane corner: half extents (1, 2) in x and z.
        assert!((started.extent - 5_f32.sqrt()).abs() < 1e-6);
        assert_eq!(started.axes, Vec3::new(1., 0., 1.));
        // The bounds centre is inside the bounds.
        assert_eq!(started.near, 0.);
        // Each 0.5 s frame advances the clock by at most `MAX_CLOCK_STEP_SECS`.
        app.update();
        assert!((effects(&app, entity).time - MAX_CLOCK_STEP_SECS).abs() < 1e-6);
        for _ in 0..100 {
            app.update();
        }
        let finished = effects(&app, entity);
        assert!(finished.enabled, "the pipeline stays specialized");
        assert!(finished.finished());
    }

    #[test]
    fn speed_stretches_load_stages_but_not_ripples() {
        let slow = at_speed(LoadEffect::Stars.stages(), 0.5);
        let normal = LoadEffect::Stars.stages();
        assert_eq!(slow.stars_duration, normal.stars_duration * 2.);
        assert_eq!(slow.duration(), normal.duration() * 2.);
        assert_eq!(slow.ripple_duration, normal.ripple_duration);
        assert_eq!(next_effect_speed(1.), 1.5);
        assert_eq!(next_effect_speed(2.), 0.5);
        // A custom rate rejoins the cycle at the next offered one.
        assert_eq!(next_effect_speed(0.6), 0.75);
        assert_eq!(parse_effect_speed("0.75"), Ok(0.75));
        assert!(parse_effect_speed("0").is_err() && parse_effect_speed("fast").is_err());
    }

    #[test]
    fn floaters_do_not_stretch_a_viewed_extent() {
        // Bounds reach 100 m out, but the camera looks at a point 3 m away.
        assert_eq!(capped_extent(100., 0., 3.), 12.);
        // A compact scene keeps its own extent.
        assert_eq!(capped_extent(5., 1., 3.), 5.);
        // A pick at the eye still animates across part of the bounds.
        assert_eq!(capped_extent(100., 0., 0.), 10.);
    }

    #[test]
    fn source_bounds_ignore_far_environment_splats() {
        let mut app = app(LoadEffect::Reveal);
        let root = app
            .world_mut()
            .spawn((
                crate::scene::SourceGeometry {
                    bounds: (Vec3::splat(-1.), Vec3::splat(1.)),
                    support: 0.,
                },
                Transform::default(),
            ))
            .id();
        // A streamed cloud's box also holds environment splats 1 km out.
        let cloud = app
            .world_mut()
            .spawn((
                CloudSettings::default(),
                Aabb::from_min_max(Vec3::splat(-1000.), Vec3::splat(1000.)),
                GlobalTransform::default(),
                InheritedVisibility::VISIBLE,
                ChildOf(root),
            ))
            .id();
        app.update();
        let started = effects(&app, cloud);
        assert_eq!(started.origin, Vec3::ZERO);
        // Farthest ground-plane corner of the source bounds, not the box.
        assert!((started.extent - 2_f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn ripples_age_out_and_evict_the_oldest() {
        let mut ripples = SplatRipples::default();
        for i in 0..RIPPLE_SLOTS + 1 {
            ripples.spawn(Vec3::splat(i as f32), 1.);
            ripples.advance(0.1, 1.);
        }
        let mut effects = SplatEffects::default();
        ripples.stamp(&mut effects);
        assert!(effects.enabled);
        // The first ripple was evicted; the rest are oldest first.
        assert_eq!(effects.ripples[0], Vec4::new(1., 1., 1., 1.));
        assert!((effects.ripple_ages.x - 0.4).abs() < 1e-6);
        ripples.advance(1., 1.);
        let mut effects = SplatEffects::default();
        ripples.stamp(&mut effects);
        assert_eq!(effects, SplatEffects::default());
    }

    #[test]
    fn ripples_play_without_a_load_effect_and_over_one() {
        for effect in [LoadEffect::None, LoadEffect::Reveal] {
            let mut app = app(effect);
            app.init_resource::<SplatRipples>();
            let entity = cloud(&mut app, true, Vec3::ZERO);
            app.update();
            app.world_mut()
                .resource_mut::<SplatRipples>()
                .spawn(Vec3::ONE, 0.5);
            app.update();
            let playing = effects(&app, entity);
            assert!(playing.enabled && playing.mask() & EFFECT_RIPPLE != 0);
            assert_eq!(playing.ripples[0], Vec4::new(1., 1., 1., 0.5));
            // Ripples never extend the load clock.
            assert_eq!(playing.duration(), effect.stages().duration());
            for _ in 0..4 {
                app.update();
            }
            assert_eq!(effects(&app, entity).mask() & EFFECT_RIPPLE, 0);
        }
    }

    #[test]
    fn generations_share_their_scene_clock_and_replay_restarts_it() {
        let mut app = app(LoadEffect::Explosion);
        let scene = app.world_mut().spawn_empty().id();
        let first = cloud(&mut app, true, Vec3::ZERO);
        app.world_mut().entity_mut(first).insert(ChildOf(scene));
        for _ in 0..3 {
            app.update();
        }
        let before = effects(&app, first).time;
        assert!(before > 0.);

        // A handoff spawns a fresh cloud; it must not restart the animation.
        let second = cloud(&mut app, true, Vec3::ZERO);
        app.world_mut().entity_mut(second).insert(ChildOf(scene));
        app.world_mut().despawn(first);
        app.update();
        assert!(effects(&app, second).time > before);

        app.world_mut().resource_mut::<SplatLoadEffect>().replay += 1;
        app.update();
        assert_eq!(effects(&app, second).time, 0.);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod gpu_tests {
    use crate::{
        renderer::readiness::Snapshot,
        streaming::runtime::raster_handoff_tests::{capture, render_app_configured, until},
    };
    use bevy::{camera::primitives::Aabb, prelude::*};
    use bevy_gaussian_splatting::{
        CloudSettings, Gaussian3d, GaussianMode, GaussianTransparency, PlanarGaussian3d,
        PlanarGaussian3dHandle, SplatEffects,
    };

    fn lit(pixels: &[u8]) -> usize {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[..3].iter().any(|&c| c > 16))
            .count()
    }

    /// Frames until two consecutive captures agree, so a pipeline swap settles.
    fn settled(app: &mut App, target: &Handle<Image>) -> Vec<u8> {
        let mut previous = capture(app, target);
        for _ in 0..60 {
            let next = capture(app, target);
            if next == previous && lit(&next) > 0 {
                return next;
            }
            previous = next;
        }
        previous
    }

    /// Splat render and sort pipelines; Bevy creates some of its own lazily.
    fn pipelines(app: &App) -> usize {
        use bevy::render::render_resource::PipelineDescriptor;
        app.sub_app(bevy::render::RenderApp)
            .world()
            .resource::<bevy::render::render_resource::PipelineCache>()
            .pipelines()
            .filter(|p| match &p.descriptor {
                PipelineDescriptor::RenderPipelineDescriptor(d) => {
                    d.label.as_deref() == Some("gaussian cloud render pipeline")
                }
                PipelineDescriptor::ComputePipelineDescriptor(d) => {
                    d.label.as_deref().is_some_and(|l| l.starts_with("radix_"))
                }
                _ => false,
            })
            .count()
    }

    #[test]
    #[ignore = "native offscreen GPU; run explicitly with --ignored --nocapture"]
    fn pipeline_warmup_compiles_every_variant_a_cloud_uses() {
        let (mut app, target, _camera) = render_app_configured(64, |_| {});
        let settings = CloudSettings {
            gaussian_mode: GaussianMode::Gaussian3d,
            transparency: GaussianTransparency::Sorted,
            transparency_warmup: true,
            effects: SplatEffects {
                enabled: true,
                stars: true,
                ..default()
            },
            ..default()
        };
        app.world_mut().spawn((
            settings.clone(),
            bevy_gaussian_splatting::CloudPipelineWarmup::default(),
        ));
        // Let the warm-up settle.
        let mut settled = pipelines(&app);
        for _ in 0..30 {
            app.update();
            let now = pipelines(&app);
            if now == settled && now > 0 {
                break;
            }
            settled = now;
        }
        let warmed = pipelines(&app);
        assert!(warmed > 0, "the warm-up compiles before any cloud exists");

        let mut rows = vec![Gaussian3d::default(); 64];
        rows[0] = Gaussian3d {
            position_visibility: [0., 0., -3., 1.].into(),
            rotation: [1., 0., 0., 0.].into(),
            scale_opacity: [0.2, 0.2, 0.2, 0.9].into(),
            ..default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<PlanarGaussian3d>>()
            .add(rows);
        let cloud = app
            .world_mut()
            .spawn((
                Snapshot,
                PlanarGaussian3dHandle(handle),
                settings,
                Transform::default(),
                Aabb::from_min_max(Vec3::splat(-4.), Vec3::splat(4.)),
            ))
            .id();
        until(&mut app, "splat renderer ready", |app| {
            app.world()
                .resource::<crate::renderer::readiness::ReadySnapshots>()
                .0
                .lock()
                .unwrap()
                .contains(&cloud)
        });
        for _ in 0..10 {
            capture(&mut app, &target);
        }
        println!(
            "pipelines: warmed {warmed}, after drawing {}",
            pipelines(&app)
        );
        assert_eq!(
            pipelines(&app),
            warmed,
            "a drawn cloud must reuse the warmed pipelines, not specialize new keys"
        );
    }

    fn set(app: &mut App, cloud: Entity, effects: SplatEffects) {
        app.world_mut()
            .get_mut::<CloudSettings>(cloud)
            .unwrap()
            .effects = effects;
    }

    #[test]
    #[ignore = "native offscreen GPU; run explicitly with --ignored --nocapture"]
    fn effects_animate_and_finish_bit_exact() {
        let (mut app, target, _camera) = render_app_configured(64, |_| {});
        let mut rows = vec![Gaussian3d::default(); 64];
        for (i, row) in rows.iter_mut().take(25).enumerate() {
            let (x, y) = ((i % 5) as f32 - 2., (i / 5) as f32 - 2.);
            *row = Gaussian3d {
                position_visibility: [x * 0.4, y * 0.4, -3., 1.].into(),
                rotation: [1., 0., 0., 0.].into(),
                scale_opacity: [0.12, 0.12, 0.12, 0.9].into(),
                ..default()
            };
            row.spherical_harmonic.coefficients[0] = 1.5;
            row.spherical_harmonic.coefficients[1] = 1.0;
        }
        let handle = app
            .world_mut()
            .resource_mut::<Assets<PlanarGaussian3d>>()
            .add(rows);
        let base = SplatEffects {
            enabled: true,
            origin: Vec3::new(0., 0., -3.),
            extent: 0.8,
            ..default()
        };
        let finished = SplatEffects {
            reveal: true,
            explosion: true,
            magic: true,
            spread: true,
            stars: true,
            time: 100.,
            // An expired ripple, inside the radius of every splat.
            ripples: [Vec4::new(0., 0., -3., 2.); 4],
            ripple_ages: Vec4::new(5., -1., -1., -1.),
            ..base
        };
        let cloud = app
            .world_mut()
            .spawn((
                Snapshot,
                PlanarGaussian3dHandle(handle),
                CloudSettings {
                    gaussian_mode: GaussianMode::Gaussian3d,
                    transparency: GaussianTransparency::Sorted,
                    effects: finished,
                    ..default()
                },
                Transform::default(),
                Aabb::from_min_max(Vec3::splat(-4.), Vec3::splat(4.)),
            ))
            .id();
        until(&mut app, "splat renderer ready", |app| {
            app.world()
                .resource::<crate::renderer::readiness::ReadySnapshots>()
                .0
                .lock()
                .unwrap()
                .contains(&cloud)
        });
        let done = settled(&mut app, &target);

        set(&mut app, cloud, SplatEffects::default());
        let plain = settled(&mut app, &target);
        println!("lit pixels: plain {} finished {}", lit(&plain), lit(&done));
        assert!(lit(&plain) > 100, "fixture must render");
        assert_eq!(
            done, plain,
            "a finished effect must match the plain pipeline"
        );

        set(
            &mut app,
            cloud,
            SplatEffects {
                explosion: true,
                ..base
            },
        );
        let mut start = Vec::new();
        for _ in 0..30 {
            start = capture(&mut app, &target);
        }
        println!("explosion t=0 lit {}", lit(&start));
        assert_eq!(lit(&start), 0, "exploding splats start invisible");

        set(
            &mut app,
            cloud,
            SplatEffects {
                reveal: true,
                ..base
            },
        );
        let hidden = settled(&mut app, &target);
        println!("reveal t=0 lit {}", lit(&hidden));
        assert!(
            lit(&hidden) < lit(&plain) / 4,
            "unrevealed splats are sparkles"
        );

        set(
            &mut app,
            cloud,
            SplatEffects {
                reveal: true,
                time: 1.2,
                ..base
            },
        );
        let sweeping = settled(&mut app, &target);
        println!("reveal mid lit {}", lit(&sweeping));
        assert_ne!(sweeping, plain);
        assert!(lit(&sweeping) > lit(&hidden));

        // Magic: nothing before the wipe, then sparkles on the swept side.
        set(
            &mut app,
            cloud,
            SplatEffects {
                magic: true,
                magic_wipe: true,
                ..base
            },
        );
        let mut unswept = Vec::new();
        for _ in 0..30 {
            unswept = capture(&mut app, &target);
        }
        println!("magic t=0 lit {}", lit(&unswept));
        assert_eq!(lit(&unswept), 0, "magic starts before the wipe");
        set(
            &mut app,
            cloud,
            SplatEffects {
                magic: true,
                magic_wipe: true,
                time: 1.05,
                ..base
            },
        );
        let wiping = settled(&mut app, &target);
        println!("magic mid-wipe lit {}", lit(&wiping));
        assert!(lit(&wiping) > 0, "swept splats sparkle");
        assert!(lit(&wiping) < lit(&plain) / 4, "swept splats are sparkles");

        // Magic front alone: a thinned field of sparkles ahead of the front.
        set(
            &mut app,
            cloud,
            SplatEffects {
                magic: true,
                ..base
            },
        );
        let sparkling = settled(&mut app, &target);
        println!("magic-front t=0 lit {}", lit(&sparkling));
        assert!(
            lit(&sparkling) < lit(&plain) / 4,
            "unsettled splats are sparse sparkles"
        );

        // Spread: nothing before the first wave, then a squeezed, partial scene.
        set(
            &mut app,
            cloud,
            SplatEffects {
                spread: true,
                ..base
            },
        );
        let mut squeezed = Vec::new();
        for _ in 0..30 {
            squeezed = capture(&mut app, &target);
        }
        println!("spread t=0 lit {}", lit(&squeezed));
        assert_eq!(lit(&squeezed), 0, "spread starts before the first wave");
        set(
            &mut app,
            cloud,
            SplatEffects {
                spread: true,
                time: 3.5,
                ..base
            },
        );
        let spreading = settled(&mut app, &target);
        println!("spread mid lit {}", lit(&spreading));
        assert!(lit(&spreading) > 0);
        assert_ne!(spreading, plain);

        // Ripple: a mid-flight ring over the grid, with no load stage playing.
        set(
            &mut app,
            cloud,
            SplatEffects {
                ripples: [Vec4::new(0., 0., -3., 2.); 4],
                ripple_ages: Vec4::new(0.3, -1., -1., -1.),
                ..base
            },
        );
        let rippling = settled(&mut app, &target);
        println!("ripple mid lit {}", lit(&rippling));
        assert_ne!(rippling, plain, "a ripple changes the frame");
        assert!(lit(&rippling) >= lit(&plain), "a ripple hides nothing");

        // Stars: every splat starts as a small point, then the sphere cuts the
        // nearer ones into full splats.
        set(
            &mut app,
            cloud,
            SplatEffects {
                stars: true,
                ..base
            },
        );
        let points = settled(&mut app, &target);
        println!("stars t=0 lit {}", lit(&points));
        assert!(lit(&points) > 0, "unrevealed splats show as points");
        // Pixel-sized points; this fixture's splats are only a few pixels wide.
        assert!(lit(&points) < lit(&plain), "points are smaller than splats");
        set(
            &mut app,
            cloud,
            SplatEffects {
                stars: true,
                time: 2.4,
                ..base
            },
        );
        let cutting = settled(&mut app, &target);
        println!("stars mid lit {}", lit(&cutting));
        assert!(lit(&cutting) > lit(&points));
        assert_ne!(cutting, plain);

        // Camera origin: a sphere around the eye reaches the nearest splats first.
        set(
            &mut app,
            cloud,
            SplatEffects {
                reveal: true,
                time: 1.5,
                origin: Vec3::ZERO,
                extent: 3.6,
                near: 2.9,
                axes: Vec3::ONE,
                ..base
            },
        );
        let sphere = settled(&mut app, &target);
        println!("camera-origin reveal mid lit {}", lit(&sphere));
        assert_ne!(sphere, plain);
        assert!(lit(&sphere) > lit(&hidden));
    }
}

#[cfg(test)]
mod camera_origin_tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .insert_resource(SplatLoadEffect {
                effect: LoadEffect::Reveal,
                origin: LoadEffectOrigin::Camera,
                ..default()
            })
            .add_systems(Update, apply_load_effect);
        let camera = app
            .world_mut()
            .spawn((
                GaussianCamera::default(),
                GlobalTransform::from_translation(Vec3::new(0., 1., 5.)),
            ))
            .id();
        let cloud = app
            .world_mut()
            .spawn((
                CloudSettings::default(),
                Aabb::from_min_max(Vec3::splat(-1.), Vec3::splat(1.)),
                GlobalTransform::default(),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        (app, camera, cloud)
    }

    fn effects(app: &App, cloud: Entity) -> SplatEffects {
        app.world().get::<CloudSettings>(cloud).unwrap().effects
    }

    fn move_camera(app: &mut App, camera: Entity, to: Vec3) {
        *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() =
            GlobalTransform::from_translation(to);
    }

    #[test]
    fn ground_effects_sweep_a_disc_around_any_origin() {
        let (mut app, _camera, cloud) = app();
        app.world_mut().resource_mut::<SplatLoadEffect>().effect = LoadEffect::Spread;
        // The first frame counts as camera motion; then 0.3 s of stillness.
        for _ in 0..4 {
            app.update();
        }
        let started = effects(&app, cloud);
        assert!(started.spread);
        assert_eq!(started.origin, Vec3::new(0., 1., 5.));
        assert_eq!(started.axes, Vec3::new(1., 0., 1.));
        // Ground distances only: farthest corner (±1, -1) from (0, 5) in xz.
        assert!((started.extent - 37_f32.sqrt()).abs() < 1e-5);
        assert!((started.near - 4.).abs() < 1e-5);
    }

    #[test]
    fn origin_waits_for_the_camera_to_settle_then_freezes() {
        let (mut app, camera, cloud) = app();
        // Animated framing: the camera moves every frame, so the clock holds.
        for step in 0..4 {
            move_camera(&mut app, camera, Vec3::new(0., 1., 5. + step as f32));
            app.update();
            assert_eq!(effects(&app, cloud).time, 0.);
        }
        // Held still, but not yet for `CAMERA_STILL_SECS`.
        app.update();
        app.update();
        assert_eq!(effects(&app, cloud).time, 0.);
        app.update();
        let started = effects(&app, cloud);
        assert_eq!(started.origin, Vec3::new(0., 1., 8.));
        assert_eq!(started.axes, Vec3::ONE);
        let farthest = Vec3::new(-1., -1., -1.).distance(Vec3::new(0., 1., 8.));
        assert!((started.extent - farthest).abs() < 1e-5);
        // The front starts at the box face nearest the eye, 7 units away.
        assert!((started.near - 7.).abs() < 1e-5);

        // Orbiting during the reveal must not move the front.
        move_camera(&mut app, camera, Vec3::new(4., 1., 0.));
        app.update();
        let later = effects(&app, cloud);
        assert_eq!(later.origin, started.origin);
        assert!(later.time > started.time);

        // Replay recaptures the camera's current position.
        app.world_mut().resource_mut::<SplatLoadEffect>().replay += 1;
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(effects(&app, cloud).origin, Vec3::new(4., 1., 0.));
    }

    #[test]
    fn a_camera_that_never_settles_starts_after_the_cap() {
        let (mut app, camera, cloud) = app();
        for step in 0..30 {
            move_camera(&mut app, camera, Vec3::new(step as f32, 1., 5.));
            app.update();
        }
        let effects = effects(&app, cloud);
        assert!(effects.time > 0., "the effect must not wait forever");
        assert_eq!(effects.axes, Vec3::ONE);
    }

    #[test]
    fn origin_names_round_trip() {
        for origin in [
            LoadEffectOrigin::Target,
            LoadEffectOrigin::Camera,
            LoadEffectOrigin::Center,
        ] {
            assert_eq!(LoadEffectOrigin::parse(origin.name()), Ok(origin));
            assert_eq!(origin.next().next().next(), origin);
        }
        assert!(LoadEffectOrigin::parse("eye").is_err());
    }
}

#[cfg(test)]
mod target_origin_tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Camera at z=5 looking down -Z at a 2x2x2 box around the origin.
    fn fixture(host: bool) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                100,
            )))
            .insert_resource(SplatLoadEffect {
                effect: LoadEffect::Reveal,
                ..default()
            })
            .add_systems(Update, apply_load_effect);
        if host {
            app.init_resource::<LoadEffectTarget>();
        }
        app.world_mut().spawn((
            GaussianCamera::default(),
            GlobalTransform::from_translation(Vec3::new(0.5, 0., 5.)),
        ));
        let cloud = app
            .world_mut()
            .spawn((
                CloudSettings::default(),
                Aabb::from_min_max(Vec3::splat(-1.), Vec3::splat(1.)),
                GlobalTransform::default(),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        (app, cloud)
    }

    fn effects(app: &App, cloud: Entity) -> SplatEffects {
        app.world().get::<CloudSettings>(cloud).unwrap().effects
    }

    #[test]
    fn a_host_pick_becomes_the_origin() {
        let (mut app, cloud) = fixture(true);
        for _ in 0..4 {
            app.update();
        }
        assert!(app.world().resource::<LoadEffectTarget>().wanted);
        assert_eq!(
            effects(&app, cloud).time,
            0.,
            "holds while the pick is pending"
        );
        app.world_mut().resource_mut::<LoadEffectTarget>().answer =
            Some(Some(Vec3::new(0.5, -1., 0.)));
        app.update();
        let started = effects(&app, cloud);
        assert_eq!(started.origin, Vec3::new(0.5, -1., 0.));
        assert_eq!(started.axes, Vec3::ONE);
        let target = app.world().resource::<LoadEffectTarget>();
        assert!(
            !target.wanted && target.answer.is_none(),
            "request is cleared"
        );
    }

    #[test]
    fn a_miss_or_no_host_uses_the_view_ray_through_the_bounds() {
        // Closest point to the centre along the ray x=0.5, y=0 looking -Z.
        let expected = Vec3::new(0.5, 0., 0.);
        let (mut app, cloud) = fixture(true);
        for _ in 0..4 {
            app.update();
        }
        app.world_mut().resource_mut::<LoadEffectTarget>().answer = Some(None);
        app.update();
        assert_eq!(effects(&app, cloud).origin, expected);

        let (mut app, cloud) = fixture(false);
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(effects(&app, cloud).origin, expected);
    }

    #[test]
    fn an_unanswered_pick_falls_back_after_the_cap() {
        let (mut app, cloud) = fixture(true);
        for _ in 0..30 {
            app.update();
        }
        let effects = effects(&app, cloud);
        assert!(effects.time > 0.);
        assert_eq!(effects.origin, Vec3::new(0.5, 0., 0.));
    }

    #[test]
    fn view_target_handles_rays_that_miss_or_start_inside() {
        let (low, high) = (Vec3::splat(-1.), Vec3::splat(1.));
        // Missing the box falls back to its centre.
        assert_eq!(
            view_target(Vec3::new(5., 5., 5.), Vec3::Z, low, high),
            Vec3::ZERO
        );
        // From inside, the closest point to the centre along the ray.
        let inside = view_target(Vec3::new(0., 0.5, 0.5), Vec3::NEG_Z, low, high);
        assert_eq!(inside, Vec3::new(0., 0.5, 0.));
    }
}
