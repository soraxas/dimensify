//! Shared native benchmark lifecycle and capture helpers.
//! Completion waits are outside app-update timing; captures remain RGBA8.

use bevy::{
    app::PluginsState,
    camera::{Hdr, RenderTarget},
    core_pipeline::{prepass::MotionVectorPrepass, tonemapping::Tonemapping},
    prelude::*,
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::{PollType, TextureFormat},
        renderer::RenderDevice,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use bevy_gaussian_splatting::{
    GaussianCamera, diagnostics::GaussianDiagnosticSample, gpu_timing::GaussianTimingView,
    render::compositing::EncodedGaussianCompositing,
};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Resource, Default)]
struct Captured(Option<Image>);

pub fn wait_for_gpu(app: &App) -> Result<(), String> {
    app.world()
        .resource::<RenderDevice>()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map(|_| ())
        .map_err(|error| format!("GPU completion failed: {error}"))
}

/// Create a windowless renderer; install workload plugins before finishing.
pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>()
            .disable::<bevy::log::LogPlugin>(),
    );
    app.add_plugins(dimensify_splat::SplatPlugin)
        .init_resource::<Captured>()
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));
    app
}

/// Finish plugins and create the common camera/target with explicit policy.
pub fn finish(
    mut app: App,
    width: u32,
    height: u32,
    timestamps: bool,
    encoded_compositing: bool,
) -> Result<(App, Handle<Image>, Entity), String> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while app.plugins_state() != PluginsState::Ready {
        if Instant::now() > deadline {
            return Err("renderer initialization timed out".into());
        }
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            width,
            height,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            GaussianCamera::default(),
            MotionVectorPrepass,
            Msaa::Off,
            Tonemapping::None,
            Projection::Perspective(PerspectiveProjection {
                fov: 45_f32.to_radians(),
                near: 0.1,
                far: 100_000.0,
                ..default()
            }),
            RenderTarget::Image(target.clone().into()),
            Transform::IDENTITY,
            GaussianDiagnosticSample(0),
        ))
        .id();
    if timestamps {
        app.world_mut()
            .entity_mut(camera)
            .insert(GaussianTimingView);
    }
    if encoded_compositing {
        app.world_mut()
            .entity_mut(camera)
            .insert((Hdr, EncodedGaussianCompositing));
    }
    Ok((app, target, camera))
}

pub fn capture(app: &mut App, target: &Handle<Image>) -> Result<Vec<u8>, String> {
    app.world_mut().resource_mut::<Captured>().0 = None;
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(
            |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                captured.0 = Some(event.image.clone());
            },
        );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.update();
        wait_for_gpu(app)?;
        if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
            return image
                .try_into_dynamic()
                .map(|image| image.into_rgba8().into_raw())
                .map_err(|error| format!("capture conversion: {error}"));
        }
        if Instant::now() > deadline {
            return Err("offscreen screenshot timed out".into());
        }
    }
}

pub fn write_ppm(path: &Path, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    write!(file, "P6\n{width} {height}\n255\n").map_err(|e| e.to_string())?;
    let rgb: Vec<_> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| pixel[..3].iter().copied())
        .collect();
    file.write_all(&rgb).map_err(|e| e.to_string())
}

pub fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn sha256(bytes: &[u8]) -> Result<String, String> {
    for (program, args) in [("shasum", vec!["-a", "256"]), ("sha256sum", vec![])] {
        let Ok(mut child) = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        child
            .stdin
            .take()
            .ok_or("hash process stdin missing")?
            .write_all(bytes)
            .map_err(|e| e.to_string())?;
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if output.status.success() {
            let hash = String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_owned();
            if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Ok(hash);
            }
        }
    }
    Err("SHA256 requires shasum or sha256sum on PATH".into())
}
