mod expectations;
#[cfg(not(target_arch = "wasm32"))]
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::{
    camera::RenderTarget,
    core_pipeline::{prepass::MotionVectorPrepass, tonemapping::Tonemapping},
    prelude::*,
    render::{
        render_resource::TextureFormat,
        renderer::RenderAdapterInfo,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    time::TimeUpdateStrategy,
};
use bevy_gaussian_splatting::{
    CloudSettings, Gaussian3d, GaussianCamera, GaussianRasterProfile, PlanarGaussian3d,
    PlanarGaussian3dHandle,
    diagnostics::{GaussianDiagnosticSample, GaussianRenderStatus},
    sort::SortMode,
};
use expectations::{CASES, SIZE};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

fn gaussian(position: [f32; 3], scale: [f32; 3], color: [f32; 3], opacity: f32) -> Gaussian3d {
    let mut g = Gaussian3d {
        position_visibility: [position[0], position[1], position[2], 1.0].into(),
        rotation: [1.0, 0.0, 0.0, 0.0].into(),
        scale_opacity: [scale[0], scale[1], scale[2], opacity].into(),
        ..default()
    };
    for (i, channel) in color.iter().enumerate() {
        g.spherical_harmonic.coefficients[i] = (channel - 0.5) / 0.2820948;
    }
    g
}
fn scene(name: &str) -> Vec<Gaussian3d> {
    match name {
        "anisotropic" => vec![gaussian(
            [0.; 3],
            [0.42, 0.10, 0.10],
            [0.9, 0.55, 0.12],
            0.85,
        )],
        "overlap-front" | "overlap-reverse" => vec![
            // Deliberately opposite input/depth orders; reversal must re-sort.
            gaussian([0., 0., 0.22], [0.25; 3], [0.95, 0.03, 0.03], 0.72),
            gaussian([0., 0., -0.22], [0.25; 3], [0.03, 0.03, 0.95], 0.72),
        ],
        "sh-front" | "sh-reverse" => {
            let mut g = gaussian([0.; 3], [0.29; 3], [0.5, 0.12, 0.5], 0.9);
            // Production lookup uses the view ray's Z component for coefficient 2.
            g.spherical_harmonic.coefficients[6] = -0.8;
            g.spherical_harmonic.coefficients[8] = 0.8;
            vec![g]
        }
        _ => unreachable!("case validated before scene construction"),
    }
}
fn pose(name: &str) -> Transform {
    Transform::from_xyz(0., 0., if name.ends_with("reverse") { -3. } else { 3. })
        .looking_at(Vec3::ZERO, Vec3::Y)
}
fn settings() -> CloudSettings {
    CloudSettings {
        sort_mode: SortMode::Radix,
        raster_profile: GaussianRasterProfile::NormalizedGaussian,
        time_scale: 0.,
        ..default()
    }
}
fn hash(value: &Value) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn metadata(world: &World, lane: &str) -> Value {
    let adapter = &world.resource::<RenderAdapterInfo>().0;
    let driver = format!("{} {}", adapter.driver, adapter.driver_info)
        .trim()
        .to_owned();
    let driver = if driver.is_empty() {
        "unavailable (adapter privacy)".to_owned()
    } else {
        driver
    };
    let adapter_name = if adapter.name.trim().is_empty() {
        "unavailable (adapter privacy)"
    } else {
        &adapter.name
    };
    let scenes: Vec<_> = CASES
        .iter()
        .map(|name| {
            json!({"name":name,"gaussians":scene(name),
        "camera":pose(name).translation.to_array()})
        })
        .collect();
    json!({"lane":lane,"target":target(),"backend":format!("{:?}",adapter.backend),
        "adapter":adapter_name,"driver":driver,
        "browser":if cfg!(target_arch="wasm32") {"chromium"} else {"none"},
        "viewport":{"width":SIZE,"height":SIZE},"sceneHash":hash(&json!(scenes)),
        "settingsHash":hash(&json!({"cloud":settings(),"size":SIZE,"fov":45,
            "format":"rgba8unorm-srgb","msaa":0,"tonemapping":"none","clear":"black","projection":{"near":0.1,"far":100.}})),
        "build":{"packageVersion":env!("CARGO_PKG_VERSION"),"debug":cfg!(debug_assertions),
            "commit":env!("DIMENSIFY_PERF_BUILD_COMMIT"),"dirty":env!("DIMENSIFY_PERF_BUILD_DIRTY"),
            "profile":env!("DIMENSIFY_PERF_BUILD_PROFILE"),"rustc":env!("DIMENSIFY_PERF_BUILD_RUSTC")}})
}
fn target() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "wasm32-unknown-unknown"
    } else if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}
fn plugins(app: &mut App) {
    let plugins = DefaultPlugins.build();
    #[cfg(not(target_arch = "wasm32"))]
    let plugins = plugins
        .disable::<PipelinedRenderingPlugin>()
        .set(WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        })
        .disable::<bevy::winit::WinitPlugin>();
    #[cfg(target_arch = "wasm32")]
    let plugins = plugins.set(WindowPlugin {
        primary_window: Some(Window {
            resolution: bevy::window::WindowResolution::new(SIZE, SIZE)
                .with_scale_factor_override(1.),
            canvas: Some("#dimensify-visual-canvas".into()),
            fit_canvas_to_parent: false,
            ..default()
        }),
        ..default()
    });
    app.add_plugins(plugins)
        .add_plugins(dimensify_splat::SplatPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1. / 60.,
        )));
}
fn setup_scene(world: &mut World, name: &str) -> (Handle<Image>, Entity, Entity) {
    let target = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            SIZE,
            SIZE,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let camera = world
        .spawn((
            Camera3d::default(),
            GaussianCamera::default(),
            MotionVectorPrepass,
            Msaa::Off,
            Tonemapping::None,
            Projection::Perspective(PerspectiveProjection {
                fov: 45_f32.to_radians(),
                near: 0.1,
                far: 100.,
                ..default()
            }),
            RenderTarget::Image(target.clone().into()),
            pose(name),
            GaussianDiagnosticSample(1),
        ))
        .id();
    let handle = world
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .add(PlanarGaussian3d::from(scene(name)));
    let cloud = world
        .spawn((
            PlanarGaussian3dHandle(handle),
            settings(),
            Transform::IDENTITY,
        ))
        .id();
    (target, camera, cloud)
}
fn acknowledged(world: &World) -> bool {
    world
        .resource::<GaussianRenderStatus>()
        .snapshots()
        .iter()
        .any(|s| {
            s.sample_id == 1
                && s.ready
                && s.cloud_count > 0
                && s.sort_encoded > 0
                && s.draw_encoded > 0
        })
}
#[derive(Resource, Default)]
struct Captured(Option<Image>);
fn request_capture(world: &mut World, target: Handle<Image>) {
    world.spawn(Screenshot::image(target)).observe(
        |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
            captured.0 = Some(event.image.clone());
        },
    );
}
fn pixels(image: Image) -> Result<Vec<u8>, String> {
    image
        .try_into_dynamic()
        .map(|i| i.into_rgba8().into_raw())
        .map_err(|e| e.to_string())
}
fn capture_row(name: &str) -> Value {
    json!({"name":name,"file":format!("{name}.png"),"width":SIZE,"height":SIZE,
        "semanticChecks":{"passed":true,"nonblank":true,"footprint":true,"centerColor":name != "anisotropic"},
        "sortEncoded":true,"drawEncoded":true,"captureCompleted":true})
}
#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
mod native;
pub fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(error) = native::run() {
        eprintln!("Visual capture failed: {error}");
        std::process::exit(1);
    }
    #[cfg(target_arch = "wasm32")]
    browser::run();
}
