//! Opt-in stochastic transparency acceptance through the production renderer.
use super::*;
use bevy_gaussian_splatting::{
    CloudSettings, Gaussian3d, PlanarGaussian3d, PlanarGaussian3dHandle,
};

fn settings(mode: &str) -> CloudSettings {
    serde_json::from_value(serde_json::json!({
        "transparency": mode, "raster_profile": "NormalizedGaussian", "time_scale": 0.0
    }))
    .unwrap()
}

fn splat(z: f32, color: [f32; 3]) -> Gaussian3d {
    let mut g = Gaussian3d {
        position_visibility: [0., 0., z, 1.].into(),
        scale_opacity: [0.8, 0.8, 0.2, 0.65].into(),
        rotation: [1., 0., 0., 0.].into(),
        ..default()
    };
    for (i, value) in color.into_iter().enumerate() {
        g.spherical_harmonic.coefficients[i] = (value - 0.5) / 0.2820948;
    }
    g
}

fn spawn(app: &mut App, mode: &str, points: Vec<Gaussian3d>) -> Entity {
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .add(PlanarGaussian3d::from(points));
    app.world_mut()
        .spawn((
            PlanarGaussian3dHandle(handle),
            settings(mode),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id()
}

fn settle(app: &mut App, target: &Handle<Image>) -> Vec<u8> {
    // Startup and newly requested pipeline specializations must finish compiling.
    until(app, "stochastic fixture pipelines", |app| {
        let cache = app
            .sub_app(bevy::render::RenderApp)
            .world()
            .resource::<bevy::render::render_resource::PipelineCache>();
        app.world().resource::<Time<Real>>().elapsed_secs() > 10.
            && cache.waiting_pipelines().count() == 0
    });
    for _ in 0..4 {
        app.update();
    }
    capture(app, target)
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn stochastic_coverage_is_binary_repeatable_and_switches_back_to_sorted() {
    let (mut app, target, _) = render_app(8);
    let entity = spawn(&mut app, "Sorted", vec![splat(-2., [1., 0., 0.])]);
    let sorted = settle(&mut app, &target);
    assert!(
        sorted
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 10 && p[0] < 240),
        "sorted Gaussian has smooth alpha"
    );
    *app.world_mut().get_mut::<CloudSettings>(entity).unwrap() = settings("Stochastic");
    let stochastic = settle(&mut app, &target);
    assert!(
        coloured_pixels(&stochastic, 0) > 50,
        "stochastic mode must draw"
    );
    assert!(
        stochastic
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] < 3 || p[0] > 252),
        "stochastic survivors must be opaque, not smooth alpha blending"
    );
    assert_eq!(
        stochastic,
        settle(&mut app, &target),
        "fixed sample must not flicker at rest"
    );
    *app.world_mut().get_mut::<CloudSettings>(entity).unwrap() = settings("Sorted");
    assert_eq!(
        sorted,
        settle(&mut app, &target),
        "returning to sorted must restore exact image"
    );
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn stochastic_overlap_uses_depth_with_partial_coverage() {
    let (mut app, target, _) = render_app(8);
    // Far blue is deliberately submitted before near red. Both must survive
    // probabilistic coverage, while a surviving near splat occludes far color.
    let entity = spawn(
        &mut app,
        "Stochastic",
        vec![splat(-3., [0., 0., 1.]), splat(-2., [1., 0., 0.])],
    );
    let pixels = settle(&mut app, &target);
    let red = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 252 && p[2] < 3)
        .count();
    let blue = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[2] > 252 && p[0] < 3)
        .count();
    assert!(
        red > 50 && blue > 10,
        "overlap requires separate opaque red and blue samples: red={red}, blue={blue}"
    );
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[0] < 3 || p[2] < 3),
        "survivors must not blend into purple"
    );
    let handle = app
        .world()
        .get::<PlanarGaussian3dHandle>(entity)
        .unwrap()
        .0
        .clone();
    *app.world_mut()
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .get_mut(&handle)
        .unwrap() =
        PlanarGaussian3d::from(vec![splat(-2., [1., 0., 0.]), splat(-3., [0., 0., 1.])]);
    let reversed = settle(&mut app, &target);
    let reverse_red = reversed
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 252 && p[2] < 3)
        .count();
    let reverse_blue = reversed
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[2] > 252 && p[0] < 3)
        .count();
    assert!(
        red.abs_diff(reverse_red) < 90 && blue.abs_diff(reverse_blue) < 90,
        "changing source order must preserve coverage distribution: {red}/{blue} versus {reverse_red}/{reverse_blue}"
    );
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn stochastic_projected_packed_lod_refines_and_returns_to_sorted() {
    use crate::{
        renderer::source::SourceStorage,
        runtime::{SogProjectionCache, SplatTransparency, apply_transparency},
    };
    use bevy_gaussian_splatting::GaussianTransparency;
    let (mut app, target, _) = render_app_configured(32, |app| {
        app.insert_resource(SogRendererBackend::Packed)
            .insert_resource(SogProjectionCache(true))
            .insert_resource(SplatTransparency(GaussianTransparency::Stochastic))
            .add_systems(PostUpdate, apply_transparency.after(update_sog_streams));
    });
    let position = Vec3::new(0., 0., -2.);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let mut data = stream(position, now);
    data.entries[0].loading = Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
    let root = app
        .world_mut()
        .spawn((data, Transform::IDENTITY, Visibility::Inherited))
        .id();
    until(&mut app, "stochastic coarse displayed", |app| {
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .displayed
            .iter()
            .any(|r| r.file == 0)
    });
    let coarse = capture(&mut app, &target);
    assert!(coloured_pixels(&coarse, 0) > 10);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    app.world_mut().get_mut::<SogStream>(root).unwrap().entries[1].loading =
        Some(chunk(position, true, now).with_storage(SourceStorage::Packed));
    until(&mut app, "stochastic fine displayed", |app| {
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .displayed
            .iter()
            .any(|r| r.file == 1)
    });
    assert!(coloured_pixels(&capture(&mut app, &target), 1) > 10);
    {
        let render = app.sub_app_mut(bevy::render::RenderApp).world_mut();
        let mut query = render.query::<&bevy_gaussian_splatting::render::compact::CompactCloud>();
        assert!(
            query
                .iter(render)
                .any(|c| c.draw_enabled() && c.has_projection_cache() && c.is_prepared()),
            "test must exercise actual projected records"
        );
    }
    app.world_mut().resource_mut::<SplatTransparency>().0 = GaussianTransparency::Sorted;
    assert!(coloured_pixels(&settle(&mut app, &target), 1) > 10);
    app.world_mut().resource_mut::<SplatTransparency>().0 = GaussianTransparency::Stochastic;
    assert!(coloured_pixels(&settle(&mut app, &target), 1) > 10);
    let mut settings = app.world_mut().query::<&CloudSettings>();
    assert!(settings.iter(app.world()).all(|s| s.stochastic()));
}
