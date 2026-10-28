//! Completed SOG transport → decode → leaf selection → real readiness → pixels.
//! Explicit offscreen GPU acceptance; never a throughput benchmark.
use super::*;
use crate::renderer::SogRendererBackend;
#[cfg(test)]
use crate::runtime::SogRuntimeSettings;
use bevy::{
    app::PluginsState,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    core_pipeline::{prepass::MotionVectorPrepass, tonemapping::Tonemapping},
    image::ImageFormat,
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        renderer::RenderAdapterInfo,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use bevy_gaussian_splatting::{GaussianCamera, GaussianSplattingPlugin};
use std::{
    collections::HashMap,
    io::Cursor,
    time::{Duration, Instant},
};

#[derive(Resource, Default)]
struct Captured(Option<Image>);

fn webp(pixel: [u8; 4], count: u32) -> Vec<u8> {
    let image = Image::new(
        Extent3d {
            width: count,
            height: 1,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixel.repeat(count as usize),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD,
    );
    let mut bytes = Cursor::new(Vec::new());
    image
        .try_into_dynamic()
        .expect("RGBA fixture")
        .write_to(
            &mut bytes,
            ImageFormat::WebP
                .as_image_crate_format()
                .expect("WebP enabled"),
        )
        .expect("encode lossless SOG texture");
    bytes.into_inner()
}

/// Build an in-memory decoded chunk for the raster handoff fixture.
pub(super) fn chunk(position: Vec3, fine: bool, now: f64) -> ChunkLoad {
    let (meta, files) = chunk_data(position, fine);
    ChunkLoad::captured(&meta, files, now)
}

fn chunk_data(position: Vec3, fine: bool) -> (Vec<u8>, HashMap<String, Vec<u8>>) {
    let count = if fine { 2 } else { 1 };
    let means = position
        .to_array()
        .map(|v| f64::from(v).signum() * (f64::from(v).abs() + 1.).ln());
    let meta = serde_json::to_vec(&serde_json::json!({
        "version": 2, "count": count,
        "means": {"mins": means, "maxs": means, "files": ["lo.webp", "hi.webp"]},
        "scales": {"codebook": [0.4_f32.ln(), 0.3_f32.ln(), 0.2_f32.ln()], "files": ["scale.webp"]},
        "quats": {"files": ["quat.webp"]},
        "sh0": {"codebook": [-1.7724539, 1.7724539], "files": ["dc.webp"]}
    }))
    .expect("fixture metadata");
    let files: HashMap<_, _> = [
        ("lo.webp", [0, 0, 0, 255]),
        ("hi.webp", [0, 0, 0, 255]),
        ("scale.webp", [0, 1, 2, 255]),
        // Smallest-three, W largest; identity to quantization tolerance.
        ("quat.webp", [128, 128, 128, 252]),
        (
            "dc.webp",
            if fine { [0, 1, 0, 255] } else { [1, 0, 0, 255] },
        ),
    ]
    .into_iter()
    .map(|(name, pixel)| (name.to_owned(), webp(pixel, count)))
    .collect();
    (meta, files)
}

#[path = "packed_handoff_tests.rs"]
#[cfg(test)]
mod packed_handoff_tests;

#[path = "turn_coverage_tests.rs"]
#[cfg(test)]
mod turn_coverage_tests;

#[test]
fn packed_chunk_loader_retains_texels_without_expanded_planes() {
    use crate::renderer::source::{SourceData, SourceStorage};
    let (meta, files) = chunk_data(Vec3::new(0., 0., -2.), false);
    let mut load = ChunkLoad::captured(&meta, files, 0.).with_storage(SourceStorage::Packed);
    let result = load
        .poll_measured(0., &SogCpuTimings::default())
        .expect("complete")
        .expect("valid source");
    assert_eq!(result.count, 1);
    assert!(
        matches!(result.cloud, SourceData::Packed(_)),
        "packed selection must not allocate an expanded cloud"
    );
}
fn stream(position: Vec3, now: f64) -> SogStream {
    // Bounds enclose 3-sigma support including quaternion quantization error.
    let half = Vec3::new(1.21, 0.91, 0.61);
    let bound =
        serde_json::json!({"min": (position-half).to_array(), "max": (position+half).to_array()});
    let bytes = serde_json::to_vec(&serde_json::json!({
        "lodLevels": 2, "filenames": ["coarse/meta.json", "fine/meta.json"],
        "tree": {"bound": bound, "lods": {
            "0": {"file": 1, "offset": 0, "count": 2},
            "1": {"file": 0, "offset": 0, "count": 1}
        }}
    }))
    .expect("manifest bytes");
    let mut stream = SogStream::parse(
        &bytes,
        "https://unused.invalid/lod-meta.json",
        "footprint".into(),
        "footprint".into(),
    )
    .expect("manifest");
    stream.entries[0].loading = Some(chunk(position, false, now));
    // Fine transport is deliberately delayed. No network is allowed in this test.
    stream.entries[1].retry_at = f64::INFINITY;
    stream
}

/// Drive the offscreen raster acceptance fixture.
pub fn until(app: &mut App, label: &str, mut ready: impl FnMut(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.update();
        if ready(app) {
            return;
        }
        if Instant::now() >= deadline {
            let mut streams = app.world_mut().query::<&SogStream>();
            for stream in streams.iter(app.world()) {
                eprintln!(
                    "active={:?}, submitted={:?}, displayed={:?}, error={:?}, submission_error={:?}",
                    stream.active,
                    stream.submitted,
                    stream.displayed,
                    stream.last_error,
                    stream.submission_error
                );
            }
            panic!("timed out: {label}");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Drive the offscreen raster acceptance fixture.
pub fn capture(app: &mut App, target: &Handle<Image>) -> Vec<u8> {
    app.world_mut().resource_mut::<Captured>().0 = None;
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(
            |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                captured.0 = Some(event.image.clone());
            },
        );
    until(app, "offscreen capture", |app| {
        app.world().resource::<Captured>().0.is_some()
    });
    app.world_mut()
        .resource_mut::<Captured>()
        .0
        .take()
        .expect("captured image")
        .try_into_dynamic()
        .expect("RGBA capture")
        .into_rgba8()
        .into_raw()
}

/// Drive the offscreen raster acceptance fixture.
pub fn coloured_pixels(pixels: &[u8], channel: usize) -> usize {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[channel] > 16 && p[channel] > p[1 - channel].saturating_add(8))
        .count()
}

#[cfg(test)]
fn empty_image(pixels: &[u8]) -> bool {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[..3] == [0, 0, 0])
}

/// Drive the offscreen raster acceptance fixture.
pub fn render_app(splat_budget: usize) -> (App, Handle<Image>, Entity) {
    render_app_configured(splat_budget, |_| {})
}

/// Configure application adapters before finishing the offscreen fixture plugins.
pub fn render_app_configured(
    splat_budget: usize,
    configure: impl FnOnce(&mut App),
) -> (App, Handle<Image>, Entity) {
    let mut app = App::new();
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        })
        .disable::<WinitPlugin>()
        .disable::<bevy::log::LogPlugin>();
    // The minimal test-support graph need not enable Bevy's multi_threaded
    // feature, which is what adds this plugin to DefaultPlugins.
    let plugins = if plugins.contains::<PipelinedRenderingPlugin>() {
        plugins.disable::<PipelinedRenderingPlugin>()
    } else {
        plugins
    };
    app.add_plugins(plugins);
    app.add_plugins(GaussianSplattingPlugin);
    renderer::readiness::install(&mut app);
    renderer::accounting::install(&mut app);
    app.init_resource::<Captured>()
        // This fixture injects expanded decoded chunks. Packed acceptance cases
        // explicitly replace the backend and supply packed source texels.
        .insert_resource(SogRendererBackend::Snapshot)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            250,
        )))
        .insert_resource(SogLodSettings {
            splat_budget,
            cache_bytes: 128 * chunk::DecodedChunk::bytes_per_splat(),
            visibility: SogVisibilityPolicy::Frustum,
            ..default()
        })
        .add_systems(Update, update_sog_streams);
    configure(&mut app);
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.plugins_state() != PluginsState::Ready {
        assert!(Instant::now() < deadline, "renderer startup timeout");
        bevy::tasks::tick_global_task_pools_on_main_thread();
    }
    app.finish();
    app.cleanup();
    eprintln!(
        "integrated SOG correctness adapter={:?}",
        app.world().resource::<RenderAdapterInfo>().0
    );
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            64,
            64,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let camera = app
        .world_mut()
        .spawn((
            StreamingCamera,
            Camera3d::default(),
            GaussianCamera::default(),
            MotionVectorPrepass,
            Msaa::Off,
            Tonemapping::None,
            Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::FRAC_PI_2,
                near: 0.1,
                ..default()
            }),
            RenderTarget::Image(target.clone().into()),
            Transform::IDENTITY,
        ))
        .id();
    // This unrelated UI camera must not become the streaming view.
    app.world_mut().spawn((
        Camera2d,
        Camera {
            is_active: false,
            ..default()
        },
        Transform::from_translation(Vec3::splat(1000.)),
    ));
    (app, target, camera)
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn encoded_compositing_changes_real_streamed_pixels_and_returns_to_linear() {
    use bevy::camera::Hdr;
    use bevy_gaussian_splatting::render::compositing::EncodedGaussianCompositing;

    let (mut app, target, camera) = render_app(8);
    // A request must keep the already-warm linear draw while its new encoded
    // specialization compiles, rather than queueing an unavailable draw.
    app.sub_app_mut(bevy::render::RenderApp).add_systems(
        bevy::render::Render,
        (|views: Query<&bevy::render::view::ExtractedView, With<EncodedGaussianCompositing>>,
          phases: Res<
            bevy::render::render_phase::ViewSortedRenderPhases<
                bevy::core_pipeline::core_3d::Transparent3d,
            >,
        >,
          cache: Res<bevy::render::render_resource::PipelineCache>| {
            for view in &views {
                if let Some(phase) = phases.get(&view.retained_view_entity) {
                    for item in phase.items.values() {
                        assert!(
                            cache.get_render_pipeline(item.pipeline).is_some(),
                            "compositing request replaced a warm draw with an unavailable pipeline"
                        );
                    }
                }
            }
        })
        .in_set(bevy::render::RenderSystems::PhaseSort),
    );
    app.world_mut().entity_mut(camera).insert(Hdr);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let root = app
        .world_mut()
        .spawn((
            stream(Vec3::new(0., 0., -2.), now),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    until(&mut app, "coarse red splat displayed", |app| {
        !app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed
            .is_empty()
    });
    let linear = capture(&mut app, &target);
    let red_sum = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| u64::from(p[0]))
            .sum::<u64>()
    };
    assert!(red_sum(&linear) > 1000);
    app.world_mut()
        .entity_mut(camera)
        .insert(EncodedGaussianCompositing);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let encoded = capture(&mut app, &target);
        // Over black, encoded alpha blending produces darker partial-coverage
        // pixels than linear blending followed by sRGB output conversion.
        if red_sum(&encoded) * 10 < red_sum(&linear) * 9 {
            assert!(
                coloured_pixels(&encoded, 0) > 0,
                "must not pass by losing the draw"
            );
            assert_eq!(
                app.world()
                    .get::<bevy_gaussian_splatting::render::compositing::CompositingFeedback>(
                        camera
                    )
                    .expect("feedback")
                    .snapshot(),
                bevy_gaussian_splatting::render::compositing::CompositingStatus::Encoded {
                    runs: 1,
                    linear_fallback_draws: 0,
                    transfer_texel_bytes: 131_072,
                },
                "report actual pass execution and 64x64 x two transfers x read/write x 8-byte texels",
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "encoded camera still uses linear blending"
        );
    }
    app.world_mut()
        .entity_mut(camera)
        .remove::<EncodedGaussianCompositing>();
    assert_eq!(
        capture(&mut app, &target),
        linear,
        "removing the request must restore normal rendering"
    );
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn encoded_compositing_keeps_unsupported_ldr_camera_on_linear_path() {
    use bevy_gaussian_splatting::render::compositing::EncodedGaussianCompositing;

    let (mut app, target, camera) = render_app(8);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let root = app
        .world_mut()
        .spawn((
            stream(Vec3::new(0., 0., -2.), now),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    until(&mut app, "coarse red splat displayed", |app| {
        !app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed
            .is_empty()
    });
    let ordinary = capture(&mut app, &target);
    assert!(coloured_pixels(&ordinary, 0) > 0);
    app.world_mut()
        .entity_mut(camera)
        .insert(EncodedGaussianCompositing);
    for _ in 0..3 {
        assert_eq!(
            capture(&mut app, &target),
            ordinary,
            "unsupported target must retain ordinary pixels"
        );
        assert_eq!(
            app.world()
                .get::<bevy_gaussian_splatting::render::compositing::CompositingFeedback>(camera)
                .expect("feedback")
                .snapshot(),
            bevy_gaussian_splatting::render::compositing::CompositingStatus::LinearFallback(
                bevy_gaussian_splatting::render::compositing::CompositingFallback::TargetFormat
            ),
        );
    }
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn encoded_compositing_preserves_msaa_oit_and_resolution_override_fallback_pixels() {
    use bevy::{
        camera::{Hdr, MainPassResolutionOverride},
        core_pipeline::oit::OrderIndependentTransparencySettings,
    };
    use bevy_gaussian_splatting::render::compositing::{
        CompositingFallback, CompositingFeedback, CompositingStatus, EncodedGaussianCompositing,
    };

    for reason in [
        CompositingFallback::Msaa,
        CompositingFallback::Oit,
        CompositingFallback::ResolutionOverride,
    ] {
        let (mut app, target, camera) = render_app(8);
        app.world_mut().entity_mut(camera).insert(Hdr);
        match reason {
            CompositingFallback::Msaa => {
                app.world_mut().entity_mut(camera).insert(Msaa::Sample4);
            }
            CompositingFallback::Oit => {
                app.world_mut()
                    .entity_mut(camera)
                    .insert(OrderIndependentTransparencySettings::default());
            }
            CompositingFallback::ResolutionOverride => {
                // Bevy consumes this component in the render world, not here.
            }
            _ => unreachable!("fixture covers only the listed camera modes"),
        }
        let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
        let root = app
            .world_mut()
            .spawn((
                stream(Vec3::new(0., 0., -2.), now),
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        until(&mut app, "fallback fixture displayed", |app| {
            !app.world()
                .get::<SogStream>(root)
                .expect("stream")
                .displayed
                .is_empty()
        });
        let render_camera = app
            .world()
            .get::<bevy::render::sync_world::RenderEntity>(camera)
            .expect("synchronized camera")
            .id();
        if reason == CompositingFallback::ResolutionOverride {
            app.sub_app_mut(bevy::render::RenderApp)
                .world_mut()
                .entity_mut(render_camera)
                .insert(MainPassResolutionOverride(UVec2::splat(32)));
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let ordinary = loop {
            let pixels = capture(&mut app, &target);
            if coloured_pixels(&pixels, 0) > 0 {
                break pixels;
            }
            assert!(
                Instant::now() < deadline,
                "ordinary {reason:?} did not render"
            );
        };
        app.world_mut()
            .entity_mut(camera)
            .insert(EncodedGaussianCompositing);
        for _ in 0..3 {
            assert_eq!(
                capture(&mut app, &target),
                ordinary,
                "{reason:?} fallback changed pixels"
            );
            assert_eq!(
                app.world()
                    .get::<CompositingFeedback>(camera)
                    .expect("feedback")
                    .snapshot(),
                CompositingStatus::LinearFallback(reason)
            );
        }
        // Removing only the incompatibility must activate the remembered request.
        app.world_mut()
            .entity_mut(camera)
            .remove::<OrderIndependentTransparencySettings>()
            .insert(Msaa::Off);
        app.sub_app_mut(bevy::render::RenderApp)
            .world_mut()
            .entity_mut(render_camera)
            .remove::<MainPassResolutionOverride>();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let pixels = capture(&mut app, &target);
            if matches!(
                app.world()
                    .get::<CompositingFeedback>(camera)
                    .expect("feedback")
                    .snapshot(),
                CompositingStatus::Encoded {
                    runs: 1,
                    linear_fallback_draws: 0,
                    ..
                }
            ) {
                assert!(
                    coloured_pixels(&pixels, 0) > 0,
                    "{reason:?} recovery lost splats"
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{reason:?} fallback did not recover"
            );
        }
    }
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn encoded_compositing_survives_resize_and_scene_unload() {
    use bevy::camera::Hdr;
    use bevy_gaussian_splatting::render::compositing::EncodedGaussianCompositing;

    let (mut app, target, camera) = render_app(8);
    app.world_mut()
        .entity_mut(camera)
        .insert((Hdr, EncodedGaussianCompositing));
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let root = app
        .world_mut()
        .spawn((
            stream(Vec3::new(0., 0., -2.), now),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    until(&mut app, "coarse splat displayed", |app| {
        !app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed
            .is_empty()
    });
    for _ in 0..5 {
        capture(&mut app, &target);
    }
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .get_mut(&target)
        .expect("target")
        .resize(Extent3d {
            width: 128,
            height: 64,
            depth_or_array_layers: 1,
        });
    app.world_mut()
        .entity_mut(camera)
        .remove::<EncodedGaussianCompositing>();
    let deadline = Instant::now() + Duration::from_secs(30);
    let ordinary = loop {
        let pixels = capture(&mut app, &target);
        if pixels.len() == 128 * 64 * 4 && coloured_pixels(&pixels, 0) > 0 {
            break pixels;
        }
        assert!(Instant::now() < deadline, "resized target did not render");
    };
    let half_red = |pixels: &[u8]| -> [u64; 2] {
        let mut red = [0, 0];
        for (index, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
            red[usize::from(index % 128 >= 64)] += u64::from(pixel[0]);
        }
        red
    };
    let baseline = half_red(&ordinary);
    assert!(baseline.iter().all(|sum| *sum > 1000));
    app.world_mut()
        .entity_mut(camera)
        .insert(EncodedGaussianCompositing);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let actual = half_red(&capture(&mut app, &target));
        if actual
            .iter()
            .zip(baseline)
            .all(|(actual, baseline)| *actual > 0 && actual * 10 < baseline * 9)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "resized transfer rectangle must cover both halves"
        );
    }
    app.world_mut().despawn(root);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if empty_image(&capture(&mut app, &target)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "unloaded scene left stale encoded pixels"
        );
    }
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn encoded_compositing_preserves_transparent_mesh_order_and_opaque_depth() {
    use bevy::camera::Hdr;
    use bevy_gaussian_splatting::render::compositing::EncodedGaussianCompositing;

    let (mut app, target, camera) = render_app(8);
    app.world_mut()
        .entity_mut(camera)
        .insert((Hdr, EncodedGaussianCompositing));
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let root = app
        .world_mut()
        .spawn((
            stream(Vec3::new(0., 0., -2.), now),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    until(&mut app, "coarse red splat displayed", |app| {
        !app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed
            .is_empty()
    });
    // Warm specialization before taking the alpha oracle: over black, the
    // encoded red channel is the production Gaussian's per-pixel alpha.
    for _ in 0..5 {
        capture(&mut app, &target);
    }
    let gaussian = capture(&mut app, &target);
    assert!(coloured_pixels(&gaussian, 0) > 0);
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::new(20., 20.));
    let material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgba(0., 0., 1., 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
    let mut quad = app
        .world_mut()
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(0., 0., -3.),
        ))
        .id();
    let encode = |v: f32| {
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    };
    let decode = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    for front in [false, true] {
        if front {
            // Use a fresh mesh for the other ordering. Bevy 0.19.1 retains
            // stale Transparent3d mesh_center after transform-only changes;
            // that independent renderer issue is outside this acceptance scenario.
            app.world_mut().despawn(quad);
            quad = app
                .world_mut()
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_xyz(0., 0., -1.),
                ))
                .id();
        }
        capture(&mut app, &target);
        let deadline = Instant::now() + Duration::from_secs(30);
        let actual = loop {
            let pixels = capture(&mut app, &target);
            if pixels[2] > 180 {
                break pixels;
            }
            assert!(Instant::now() < deadline, "transparent mesh did not render");
        };
        for (base, actual) in gaussian
            .as_chunks::<4>()
            .0
            .iter()
            .zip(actual.as_chunks::<4>().0.iter())
        {
            let alpha = f32::from(base[0]) / 255.;
            if alpha < 0.05 {
                continue;
            }
            let expected = if front {
                [encode(decode(alpha) * 0.5), 0., encode(0.5)]
            } else {
                [alpha, 0., encode(0.5) * (1. - alpha)]
            };
            for channel in 0..3 {
                assert!(
                    (f32::from(actual[channel]) - expected[channel] * 255.).abs() < 3.,
                    "front={front} alpha={alpha} actual={actual:?} expected={expected:?}"
                );
            }
        }
    }
    {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let mut material = materials.get_mut(&material).expect("quad material");
        material.alpha_mode = AlphaMode::Opaque;
        material.base_color = Color::srgb(0., 0., 1.);
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let pixels = capture(&mut app, &target);
        if pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[..3] == [0, 0, 255])
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "opaque mesh must occlude encoded Gaussians"
        );
    }
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn encoded_compositing_isolates_cameras_sharing_a_target() {
    use bevy::camera::{Hdr, Viewport};
    use bevy_gaussian_splatting::render::compositing::EncodedGaussianCompositing;

    let (mut app, target, camera) = render_app(8);
    app.world_mut().entity_mut(camera).insert((
        Hdr,
        Camera {
            viewport: Some(Viewport {
                physical_size: UVec2::new(32, 64),
                ..default()
            }),
            ..default()
        },
    ));
    app.world_mut().spawn((
        Camera3d::default(),
        GaussianCamera::default(),
        MotionVectorPrepass,
        Hdr,
        Msaa::Off,
        Tonemapping::None,
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            near: 0.1,
            ..default()
        }),
        Camera {
            order: 1,
            viewport: Some(Viewport {
                physical_position: UVec2::new(32, 0),
                physical_size: UVec2::new(32, 64),
                ..default()
            }),
            ..default()
        },
        RenderTarget::Image(target.clone().into()),
        Transform::IDENTITY,
    ));
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let root = app
        .world_mut()
        .spawn((
            stream(Vec3::new(0., 0., -2.), now),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    until(&mut app, "coarse red splat displayed", |app| {
        !app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed
            .is_empty()
    });
    let halves = |pixels: &[u8]| -> [Vec<u8>; 2] {
        let mut halves = [Vec::new(), Vec::new()];
        for row in pixels.as_chunks::<{ 64 * 4 }>().0 {
            halves[0].extend_from_slice(&row[..32 * 4]);
            halves[1].extend_from_slice(&row[32 * 4..]);
        }
        halves
    };
    let ordinary = halves(&capture(&mut app, &target));
    assert!(ordinary.iter().all(|half| coloured_pixels(half, 0) > 0));
    app.world_mut()
        .entity_mut(camera)
        .insert(EncodedGaussianCompositing);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let actual = halves(&capture(&mut app, &target));
        assert_eq!(
            actual[1], ordinary[1],
            "unrequested sibling camera must remain linear"
        );
        if actual[0] != ordinary[0] {
            assert!(coloured_pixels(&actual[0], 0) > 0);
            break;
        }
        assert!(Instant::now() < deadline, "requested view did not change");
    }
    app.world_mut()
        .entity_mut(camera)
        .remove::<EncodedGaussianCompositing>();
    assert_eq!(halves(&capture(&mut app, &target)), ordinary);
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn default_screen_space_zoom_out_and_return_restore_rendered_fine_detail() {
    let (mut app, target, camera) = render_app(8);
    assert_eq!(
        app.world().resource::<SogLodSettings>().lod_policy,
        crate::streaming::SogLodPolicy::ScreenSpace
    );
    let position = Vec3::new(0., 0., -2.);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let mut fixture = stream(position, now);
    fixture.entries[1].loading = Some(chunk(position, true, now));
    let root = app
        .world_mut()
        .spawn((fixture, Transform::IDENTITY, Visibility::Inherited))
        .id();
    let fine = vec![SplatRange {
        file: 1,
        offset: 0,
        count: 2,
    }];
    until(&mut app, "default fine selection", |app| {
        app.world().get::<SogStream>(root).unwrap().displayed == fine
    });
    let initial = capture(&mut app, &target);
    assert!(
        coloured_pixels(&initial, 1) > 0,
        "fine green fixture must render"
    );
    app.world_mut()
        .entity_mut(camera)
        .insert(Transform::from_xyz(0., 0., 100.));
    until(&mut app, "zoom-out coarsens rendered selection", |app| {
        app.world().get::<SogStream>(root).unwrap().displayed
            == [SplatRange {
                file: 0,
                offset: 0,
                count: 1,
            }]
    });
    assert_ne!(capture(&mut app, &target), initial);
    app.world_mut()
        .entity_mut(camera)
        .insert(Transform::IDENTITY);
    until(&mut app, "cached return restores fine selection", |app| {
        app.world().get::<SogStream>(root).unwrap().displayed == fine
    });
    assert_eq!(
        capture(&mut app, &target),
        initial,
        "cached return must restore actual rendered pixels"
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .entries
            .iter()
            .all(|entry| entry.loading.is_none())
    );
}

#[test]
#[ignore = "native offscreen GPU integration; run explicitly with --ignored --nocapture"]
fn decoded_leaf_footprints_survive_coarse_to_fine_render_handoff() {
    let (mut app, target, camera) = render_app(8);
    // This fixture compares configured All versus Frustum. Screen-space always
    // applies effective frustum selection, so keep the reference baseline explicit.
    app.world_mut().resource_mut::<SogLodSettings>().lod_policy =
        crate::streaming::SogLodPolicy::ReferenceDistance;
    for (backend, compaction) in [
        (SogRendererBackend::Snapshot, false),
        (SogRendererBackend::GpuResident, false),
        (SogRendererBackend::GpuResident, true),
    ] {
        app.insert_resource(backend);
        app.world_mut()
            .resource_mut::<SogRuntimeSettings>()
            .compaction = compaction;
        for (label, position) in [
            ("centre", Vec3::new(0., 0., -2.)),
            ("right edge", Vec3::new(2.4, 0., -2.)),
            ("left edge", Vec3::new(-2.4, 0., -2.)),
            ("upper edge", Vec3::new(0., 2.4, -2.)),
            ("lower edge", Vec3::new(0., -2.4, -2.)),
            ("near plane", Vec3::new(0., 0., -0.05)),
        ] {
            // A common rigid transform should leave the projected fixture alone;
            // selection must use local space while rendering uses the real root.
            let transform =
                Transform::from_xyz(7., -3., 4.).with_rotation(Quat::from_rotation_y(0.7));
            app.world_mut().entity_mut(camera).insert(transform);
            let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
            let root = app
                .world_mut()
                .spawn((stream(position, now), transform, Visibility::Inherited))
                .id();
            for file in [0, 1] {
                app.world_mut().resource_mut::<SogLodSettings>().visibility =
                    SogVisibilityPolicy::All;
                if file == 1 {
                    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
                    app.world_mut()
                        .get_mut::<SogStream>(root)
                        .expect("stream")
                        .entries[1]
                        .loading = Some(chunk(position, true, now));
                }
                let expected = [SplatRange {
                    file,
                    offset: 0,
                    count: file + 1,
                }];
                until(&mut app, label, |app| {
                    app.world()
                        .get::<SogStream>(root)
                        .expect("stream")
                        .displayed
                        == expected
                });
                let baseline = capture(&mut app, &target);
                let allocation = app
                    .world()
                    .resource::<renderer::accounting::SortGpuAccounting>()
                    .telemetry()
                    .sort_gpu_buffers
                    .expect("render-observed sorting allocations");
                assert!(
                    allocation.sorted_entries.live_assets > 0
                        && allocation.sorted_entries.live_bytes >= 32 * 8,
                    "rendered selection lacks sort-entry accounting: {backend:?}, file={file}, {allocation:?}"
                );
                assert!(
                    allocation.radix_scratch.live_assets > 0
                        && allocation.radix_scratch.live_buffers >= 8,
                    "rendered selection lacks radix accounting: {backend:?}, file={file}, {allocation:?}"
                );
                let channel = file; // coarse red, fine green proves that replacement rendered.
                let visible = coloured_pixels(&baseline, channel);
                assert!(
                    visible > 0,
                    "all-leaf baseline lost decoded support: {label}, {backend:?}, compaction={compaction}, file={file}"
                );
                app.world_mut().resource_mut::<SogLodSettings>().visibility =
                    SogVisibilityPolicy::Frustum;
                // Force a policy tick; an unchanged displayed range alone does
                // not prove the new policy was consumed.
                app.world_mut()
                    .get_mut::<SogStream>(root)
                    .expect("stream")
                    .next_update = 0.;
                app.update();
                assert_eq!(
                    app.world().get::<SogStream>(root).expect("stream").active,
                    expected
                );
                let pixels = capture(&mut app, &target);
                assert!(
                    pixels == baseline,
                    "leaf culling changed visible pixels: {label}, {backend:?}, compaction={compaction}, file={file}"
                );
            }
            if label == "right edge" {
                // The same decoded, resident source must disappear/reappear
                // through the real camera/selection/render path, with no refetch.
                let projection = app
                    .world()
                    .get::<Projection>(camera)
                    .expect("projection")
                    .clone();
                for change_fov in [false, true] {
                    if change_fov {
                        app.world_mut()
                            .entity_mut(camera)
                            .insert(Projection::Perspective(PerspectiveProjection {
                                fov: 20_f32.to_radians(),
                                near: 0.1,
                                ..default()
                            }));
                    } else {
                        app.world_mut().entity_mut(camera).insert(Transform {
                            translation: transform.translation
                                + transform.rotation * Vec3::new(50., 0., 0.),
                            ..transform
                        });
                    }
                    until(
                        &mut app,
                        "camera reduces offscreen leaf to coarse coverage",
                        |app| {
                            app.world()
                                .get::<SogStream>(root)
                                .expect("stream")
                                .displayed
                                == [SplatRange {
                                    file: 0,
                                    offset: 0,
                                    count: 1,
                                }]
                        },
                    );
                    assert!(
                        empty_image(&capture(&mut app, &target)),
                        "camera change left stale pixels: fov={change_fov}, {backend:?}, compaction={compaction}"
                    );
                    app.world_mut()
                        .entity_mut(camera)
                        .insert((transform, projection.clone()));
                    until(&mut app, "camera returns to fine leaf", |app| {
                        app.world()
                            .get::<SogStream>(root)
                            .expect("stream")
                            .displayed
                            == [SplatRange {
                                file: 1,
                                offset: 0,
                                count: 2,
                            }]
                    });
                    assert!(
                        coloured_pixels(&capture(&mut app, &target), 1) > 0,
                        "camera return lost fine pixels"
                    );
                    assert!(
                        app.world()
                            .get::<SogStream>(root)
                            .expect("stream")
                            .entries
                            .iter()
                            .all(|entry| entry.loading.is_none()),
                        "resident return unexpectedly fetched data"
                    );
                }
            }
            app.world_mut().entity_mut(root).despawn();
            app.update();
            assert!(
                empty_image(&capture(&mut app, &target)),
                "removed leaf left stale raster pixels"
            );
            until(
                &mut app,
                "retired scene releases observed sorting allocations",
                |app| {
                    app.world()
                        .resource::<renderer::accounting::SortGpuAccounting>()
                        .telemetry()
                        .sort_gpu_buffers
                        .is_some_and(|allocation| {
                            allocation.sorted_entries.live_bytes == 0
                                && allocation.radix_scratch.live_bytes == 0
                        })
                },
            );
        }
    }
}

#[cfg(test)]
mod producer_bounds;

/// Build a captured red or green chunk without performing network requests.
pub fn captured_stream(position: Vec3, fine: bool, now: f64, storage: SourceStorage) -> SogStream {
    let mut value = stream(position, now);
    value.entries[0].loading = Some(chunk(position, fine, now).with_storage(storage));
    value
}
