//! Opt-in load animations, played once when each scene first becomes visible.
//!
//! The renderer evaluates the stages per splat (`render/effects.wesl` in the
//! vendored renderer). This module owns the per-scene clock and bounds: clouds
//! are replaced as streamed generations hand off, so the clock, origin and
//! extent belong to the scene root and are stamped onto every current cloud.
use bevy::{camera::primitives::Aabb, prelude::*};
use bevy_gaussian_splatting::{CloudSettings, GaussianCamera, SplatEffects};
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
}

impl LoadEffect {
    pub const ALL: [Self; 4] = [
        Self::None,
        Self::Reveal,
        Self::Explosion,
        Self::RevealExplosion,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Reveal => "reveal",
            Self::Explosion => "explosion",
            Self::RevealExplosion => "reveal-explosion",
        }
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        Self::ALL
            .into_iter()
            .find(|effect| effect.name() == value)
            .ok_or("effect must be none, reveal, explosion or reveal-explosion")
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|&e| e == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    fn stages(self) -> (bool, bool) {
        match self {
            Self::None => (false, false),
            Self::Reveal => (true, false),
            Self::Explosion => (false, true),
            Self::RevealExplosion => (true, true),
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
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SplatLoadEffect {
    pub effect: LoadEffect,
    pub origin: LoadEffectOrigin,
    /// Increment to restart every scene's animation (recapturing the camera).
    pub replay: u32,
}

/// Look-at handshake for [`LoadEffectOrigin::Target`]. `apply_load_effect`
/// sets `wanted` while a scene waits for its origin; a host with a surface pick
/// (the viewer's depth pick at the view centre) writes `answer`: `Some(hit)`
/// or `None` for a miss. Without an answer the bounds estimate is used.
#[derive(Resource, Default, Debug)]
pub struct LoadEffectTarget {
    pub wanted: bool,
    pub answer: Option<Option<Vec3>>,
}

/// A camera or target origin waits this long at most for the camera to
/// settle, e.g. after the viewer's animated frame-on-load.
const CAMERA_SETTLE_SECS: f32 = 1.0;
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
    /// Seconds each visible scene has waited for the camera to settle.
    waiting: HashMap<Entity, f32>,
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

#[allow(clippy::type_complexity)]
pub fn apply_load_effect(
    config: Res<SplatLoadEffect>,
    time: Res<Time>,
    mut state: Local<LoadEffectState>,
    parents: Query<&ChildOf>,
    cameras: Query<&GlobalTransform, With<GaussianCamera>>,
    mut target: Option<ResMut<LoadEffectTarget>>,
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
    let (reveal, explosion) = config.effect.stages();
    let template = SplatEffects {
        enabled: reveal || explosion,
        reveal,
        explosion,
        ..default()
    };

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
    state.scenes.retain(|root, _| live.contains(root));
    state.waiting.retain(|root, _| live.contains(root));
    let delta = time.delta_secs();
    let duration = template.duration();
    for playback in state.scenes.values_mut() {
        playback.time = (playback.time + delta).min(duration);
    }
    let mut want_target = false;
    if template.enabled {
        for (root, (low, high)) in visible {
            if state.scenes.contains_key(&root) {
                continue;
            }
            let (origin, mode) = match (config.origin, view) {
                (LoadEffectOrigin::Center, _) | (_, None) => {
                    ((low + high) * 0.5, LoadEffectOrigin::Center)
                }
                (mode, Some((eye, forward))) => {
                    // Hold the first frame while the camera is still moving.
                    let waited = state.waiting.entry(root).or_default();
                    *waited += delta;
                    let waited = *waited;
                    if camera_moved && waited < CAMERA_SETTLE_SECS {
                        continue;
                    }
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
            let axes = mode.axes();
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
        let effects = match state.scenes.get(&root) {
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
        if settings.effects != effects {
            settings.effects = effects;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
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
                replay: 0,
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
        assert_eq!(LoadEffect::RevealExplosion.next(), LoadEffect::None);
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
        for _ in 0..20 {
            app.update();
        }
        let finished = effects(&app, entity);
        assert!(finished.enabled, "the pipeline stays specialized");
        assert!(finished.finished());
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
            time: 100.,
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
    fn origin_waits_for_the_camera_to_settle_then_freezes() {
        let (mut app, camera, cloud) = app();
        // Animated framing: the camera moves every frame, so the clock holds.
        for step in 0..4 {
            move_camera(&mut app, camera, Vec3::new(0., 1., 5. + step as f32));
            app.update();
            assert_eq!(effects(&app, cloud).time, 0.);
        }
        app.update();
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
        app.update();
        app.update();
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
        for _ in 0..3 {
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
        for _ in 0..3 {
            app.update();
        }
        app.world_mut().resource_mut::<LoadEffectTarget>().answer = Some(None);
        app.update();
        assert_eq!(effects(&app, cloud).origin, expected);

        let (mut app, cloud) = fixture(false);
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(effects(&app, cloud).origin, expected);
    }

    #[test]
    fn an_unanswered_pick_falls_back_after_the_cap() {
        let (mut app, cloud) = fixture(true);
        for _ in 0..25 {
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
