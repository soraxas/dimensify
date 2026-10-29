//! Manual offscreen measurement of the production Gaussian pipeline.
//! Every measured update is followed by a GPU wait; no display FPS is inferred.
#[path = "../gpu/camera.rs"]
mod camera;
#[path = "../gpu/options.rs"]
mod options;
#[path = "../gpu/stats.rs"]
mod stats;
#[path = "../gpu/validation.rs"]
mod validation;

use bevy::{
    app::PluginsState,
    camera::RenderTarget,
    core_pipeline::{prepass::MotionVectorPrepass, tonemapping::Tonemapping},
    prelude::*,
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::{PollType, TextureFormat},
        renderer::{RenderAdapterInfo, RenderDevice},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    time::TimeUpdateStrategy,
    window::ExitCondition,
    winit::WinitPlugin,
};
use bevy_gaussian_splatting::{
    CloudSettings, GaussianCamera, GaussianRasterProfile, PlanarGaussian3d, PlanarGaussian3dHandle,
    diagnostics::{GaussianDiagnosticSample, GaussianRenderStatus},
    gpu_timing::{
        GaussianGpuTimingPlugin, GaussianGpuTimings, GaussianTimingSource, GaussianTimingView,
    },
    sort::SortMode,
};
use options::Options;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HELP: &str = "Usage: gpu [scene.ply|scene.sog] [--camera SOURCE_CAMERA.json] [--output DIRECTORY] [--width 1280] [--height 720] [--samples 240] [--warmup 32] [--gpu-timestamps] [--smoke]\nNo file generates a deterministic 65,536-splat PLY with SH3. Writes raw.json, bmf.json and four PPM captures. Sampling covers at least 240 frames by default and two seconds of completed work. Output directory must be new. Publication requires a clean, matching optimized build and source checkout on a hardware adapter. --smoke permits software adapters and suppresses BMF publication. GPU timestamps are optional radix/compaction diagnostics, never total render time.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return;
    }
    if let Err(error) = Options::parse(args).and_then(run) {
        eprintln!("GPU benchmark failed: {error}");
        std::process::exit(1);
    }
}

#[derive(Resource, Default)]
struct Captured(Option<Image>);

fn wait_for_gpu(app: &App) -> Result<(), String> {
    app.world()
        .resource::<RenderDevice>()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map(|_| ())
        .map_err(|error| format!("GPU completion failed: {error}"))
}

fn render_frame(app: &mut App, camera: Entity, cloud: Entity, id: u64) -> Result<Value, String> {
    app.world_mut()
        .entity_mut(camera)
        .insert(GaussianDiagnosticSample(id));
    app.world_mut()
        .entity_mut(cloud)
        .insert(GaussianTimingSource {
            scene: 1,
            selection: id,
        });
    let start = Instant::now();
    app.update();
    let update_ns = start.elapsed().as_nanos() as f64;
    wait_for_gpu(app)?;
    let completion_ns = start.elapsed().as_nanos() as f64;
    let status = app.world().resource::<GaussianRenderStatus>().snapshots();
    let acknowledged = status.iter().any(|sample| {
        sample.sample_id == id
            && sample.ready
            && sample.cloud_count > 0
            && sample.sort_encoded > 0
            && sample.draw_encoded > 0
    });
    Ok(
        json!({"sample_id":id,"app_update_wall_ns":update_ns,"completed_frame_ns":completion_ns,
        "acknowledged_sort_and_draw":acknowledged,
        "sort_encoded":status.iter().filter(|s|s.sample_id==id).map(|s|s.sort_encoded).sum::<usize>(),
        "draw_encoded":status.iter().filter(|s|s.sample_id==id).map(|s|s.draw_encoded).sum::<usize>()}),
    )
}

fn setup(options: &Options) -> Result<(App, Handle<Image>, Entity), String> {
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
    if options.timestamps {
        app.add_plugins(GaussianGpuTimingPlugin);
    }
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
            options.width,
            options.height,
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
    if options.timestamps {
        app.world_mut()
            .entity_mut(camera)
            .insert(GaussianTimingView);
    }
    Ok((app, target, camera))
}

fn camera_transform(
    center: Vec3,
    distance: f32,
    side: bool,
    moving: bool,
    frame: usize,
    count: usize,
) -> Transform {
    let base_angle = if side { 0.55 } else { 0.0 };
    let angle = base_angle
        + if moving {
            std::f32::consts::FRAC_PI_4
                * (frame as f32 / count as f32 * std::f32::consts::TAU).sin()
        } else {
            0.0
        };
    let radius = distance * if side { 0.65 } else { 1.0 };
    Transform::from_translation(center + Vec3::new(angle.sin() * radius, 0.0, angle.cos() * radius))
        .looking_at(center, Vec3::Y)
}

fn capture(app: &mut App, target: &Handle<Image>) -> Result<Vec<u8>, String> {
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

fn gpu_samples(app: &App, ids: &BTreeSet<u64>, output: &mut BTreeMap<u64, Value>) {
    let Some(bridge) = app.world().get_resource::<GaussianGpuTimings>() else {
        return;
    };
    if let Some(report) = bridge.report() {
        for sample in &report.samples {
            if ids.contains(&sample.identity.selection) {
                output.insert(
                    sample.identity.selection,
                    serde_json::to_value(sample).expect("timing is serializable"),
                );
            }
        }
    }
}

fn write_ppm(path: &Path, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
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

fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn sha256(bytes: &[u8]) -> Result<String, String> {
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

fn run(options: Options) -> Result<(), String> {
    let git_args = ["-C", env!("CARGO_MANIFEST_DIR"), "rev-parse", "HEAD"];
    let dirty_args = ["-C", env!("CARGO_MANIFEST_DIR"), "status", "--porcelain"];
    let initial_commit = command("git", &git_args);
    let initial_dirty = command("git", &dirty_args).map(|s| !s.is_empty());
    validation::create_output(&options.output)?;
    let executable = std::env::current_exe().map_err(|e| format!("executable path: {e}"))?;
    let executable_sha256 =
        sha256(&fs::read(&executable).map_err(|e| format!("read executable: {e}"))?)?;
    let prepare_start = Instant::now();
    let (name, bytes) = if let Some(path) = &options.input {
        (
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or("source filename is not UTF-8")?
                .to_owned(),
            fs::read(path).map_err(|e| format!("read source: {e}"))?,
        )
    } else {
        (
            "generated-65536-sh3.ply".into(),
            dimensify_perf::pipeline::binary_ply(65_536),
        )
    };
    let source_prepare_ns = prepare_start.elapsed().as_nanos();
    let hash = sha256(&bytes)?;
    let decode_start = Instant::now();
    let decoded = dimensify_splat::decode_splat(&name, &bytes)?;
    let decode_ns = decode_start.elapsed().as_nanos();
    if decoded.count == 0 {
        return Err("source contains no Gaussians".into());
    }
    let count = decoded.count;
    let storage_rows = decoded.cloud.position_visibility.len();
    let center = (decoded.bounds.0 + decoded.bounds.1) * 0.5;
    let support = decoded
        .cloud
        .scale_opacity
        .iter()
        .take(count)
        .flat_map(|row| row.scale)
        .fold(0.0_f32, f32::max)
        * 3.0;
    let radius = ((decoded.bounds.1 - decoded.bounds.0).length() * 0.5 + support).max(0.1);
    let distance = radius / (22.5_f32.to_radians().sin()) * 1.12;
    let explicit_camera = options
        .camera
        .as_ref()
        .map(|path| {
            let bytes = fs::read(path).map_err(|e| format!("read camera: {e}"))?;
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| format!("camera JSON: {e}"))?;
            let path = camera::CameraPath::from_json(&value)?;
            Ok::<_, String>((path, value, sha256(&bytes)?))
        })
        .transpose()?;
    let transform = |side, moving, frame| {
        explicit_camera.as_ref().map_or_else(
            || camera_transform(center, distance, side, moving, frame, options.samples),
            |(path, _, _)| path.transform(side, moving, frame, options.samples),
        )
    };
    let settings = CloudSettings {
        sort_mode: SortMode::Radix,
        raster_profile: GaussianRasterProfile::NormalizedGaussian,
        ..default()
    };
    let effective_settings = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    let setup_start = Instant::now();
    let (mut app, target, camera) = setup(&options)?;
    let setup_ns = setup_start.elapsed().as_nanos();
    let adapter = app.world().resource::<RenderAdapterInfo>().0.clone();
    let device_type = format!("{:?}", adapter.device_type);
    validation::validate_adapter(&device_type, &adapter.name, options.smoke)?;
    let device = app.world().resource::<RenderDevice>();
    let features = format!("{:?}", device.features());
    let limits = format!("{:?}", device.limits());
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .add(decoded.cloud);
    let cloud = app
        .world_mut()
        .spawn((
            PlanarGaussian3dHandle(handle),
            settings,
            GaussianTimingSource {
                scene: 1,
                selection: 0,
            },
            Transform::IDENTITY,
        ))
        .id();
    let mut cases = Vec::new();
    let mut bmf = serde_json::Map::new();
    let mut publication = validation::PublicationEvidence {
        smoke: options.smoke,
        debug: cfg!(debug_assertions),
        hardware: validation::hardware_adapter(&device_type, &adapter.name),
        build_commit: option_env!("DIMENSIFY_PERF_BUILD_COMMIT"),
        build_dirty: option_env!("DIMENSIFY_PERF_BUILD_DIRTY").and_then(|s| s.parse().ok()),
        build_opt_level: option_env!("DIMENSIFY_PERF_BUILD_OPT_LEVEL"),
        runtime_commit: initial_commit.as_deref(),
        runtime_dirty: initial_dirty,
    };
    let mut next_id = 0_u64;
    for (side, moving, label) in [
        (false, false, "main-static"),
        (false, true, "main-moving"),
        (true, false, "side-static"),
        (true, true, "side-moving"),
    ] {
        eprintln!("GPU {label}: warming actual sort and draw");
        if options.timestamps {
            app.world_mut()
                .entity_mut(camera)
                .insert(GaussianTimingView);
        }
        let warm_start = Instant::now();
        let mut first_ready = None;
        let mut first_acknowledged_ns = None;
        let mut consecutive = 0;
        let mut warm_frames = 0;
        loop {
            *app.world_mut().get_mut::<Transform>(camera).unwrap() =
                transform(side, moving, warm_frames);
            next_id += 1;
            let row = render_frame(&mut app, camera, cloud, next_id)?;
            warm_frames += 1;
            if row["acknowledged_sort_and_draw"] == true {
                consecutive += 1;
                first_acknowledged_ns.get_or_insert_with(|| warm_start.elapsed().as_nanos());
                first_ready.get_or_insert_with(Instant::now);
            } else {
                consecutive = 0;
                first_ready = None;
            }
            if consecutive >= options.warmup
                && first_ready.is_some_and(|t| t.elapsed() >= Duration::from_secs(1))
            {
                break;
            }
            if warm_start.elapsed() > Duration::from_secs(60) {
                return Err(format!(
                    "{label}: no stable nonzero sort+draw within 60s; last row {row}"
                ));
            }
        }
        let warm_ns = warm_start.elapsed().as_nanos();
        let mut rows = Vec::with_capacity(options.samples);
        let mut ids = BTreeSet::new();
        let mut timings = BTreeMap::new();
        let mut total_completion_ns = 0.0;
        let mut frame = 0;
        while frame < options.samples
            || total_completion_ns < 2_000_000_000.0
            || !frame.is_multiple_of(options.samples)
        {
            *app.world_mut().get_mut::<Transform>(camera).unwrap() = transform(side, moving, frame);
            next_id += 1;
            ids.insert(next_id);
            let mut row = render_frame(&mut app, camera, cloud, next_id)?;
            if row["acknowledged_sort_and_draw"] != true {
                return Err(format!(
                    "{label}: measured frame omitted real sort/draw: {row}"
                ));
            }
            let pose = app.world().get::<Transform>(camera).unwrap();
            row["camera_position"] = json!(pose.translation.to_array());
            row["camera_rotation_xyzw"] = json!(pose.rotation.to_array());
            total_completion_ns += row["completed_frame_ns"].as_f64().unwrap();
            rows.push(row);
            frame += 1;
            gpu_samples(&app, &ids, &mut timings);
        }
        // Timestamp resolve is deferred by the recorder. Drain outside timed frames,
        // with sampling disabled, retaining original selection IDs in the results.
        app.world_mut()
            .entity_mut(camera)
            .remove::<GaussianTimingView>();
        for _ in 0..2 {
            app.update();
            wait_for_gpu(&app)?;
            gpu_samples(&app, &ids, &mut timings);
        }
        let mut radix_ns = Vec::new();
        for row in &mut rows {
            let id = row["sample_id"].as_u64().unwrap();
            let sample = timings.get(&id);
            row["gpu_radix_sample"] = sample.cloned().unwrap_or(Value::Null);
            if let Some(ms) = sample.and_then(|s| s["measurement"]["duration_ms"].as_f64()) {
                radix_ns.push(ms * 1_000_000.0);
            }
        }
        let pixels = capture(&mut app, &target)?;
        let non_background = validation::validate_pixels(&pixels, options.width, options.height)?;
        let capture_name = format!("{label}.ppm");
        write_ppm(
            &options.output.join(&capture_name),
            &pixels,
            options.width,
            options.height,
        )?;
        let completed: Vec<_> = rows
            .iter()
            .map(|r| r["completed_frame_ns"].as_f64().unwrap())
            .collect();
        let update: Vec<_> = rows
            .iter()
            .map(|r| r["app_update_wall_ns"].as_f64().unwrap())
            .collect();
        let mut summary = json!({"completed_frame":stats::distribution(&completed)?,"app_update_wall":stats::distribution(&update)?,
            "gpu_radix":null});
        if !radix_ns.is_empty() {
            summary["gpu_radix"] = stats::distribution(&radix_ns)?;
        }
        if validation::publication_eligible(&publication) {
            for (metric, values) in [
                ("completed_frame", &completed),
                ("app_update_wall", &update),
                ("gpu_radix", &radix_ns),
            ] {
                if values.is_empty() {
                    continue;
                }
                let key = format!(
                    "splat/native-offscreen/{}/{:}x{:}/sh3-normalized-sort32/{label}/{metric}",
                    &hash[..12],
                    options.width,
                    options.height
                );
                let key = if let Some((_, _, camera_hash)) = &explicit_camera {
                    format!("{key}/camera-{}", &camera_hash[..12])
                } else {
                    key
                };
                bmf.insert(key.clone(), stats::bmf_metric(values)?);
                let distribution = stats::distribution(values)?;
                for tail in ["p95", "p99"] {
                    bmf.insert(
                        format!("{key}/{tail}"),
                        json!({"latency":{"value":distribution[format!("{tail}_ns")]}}),
                    );
                }
            }
        }
        eprintln!(
            "GPU {label}: median {:.3} ms, P95 {:.3} ms; {} non-background pixels",
            summary["completed_frame"]["median_ns"].as_f64().unwrap() / 1_000_000.0,
            summary["completed_frame"]["p95_ns"].as_f64().unwrap() / 1_000_000.0,
            non_background
        );
        cases.push(json!({"name":label,"moving":moving,"side":side,"warmup_frames":warm_frames,
            "warmup_ns":warm_ns,"first_acknowledged_completed_draw_ns":first_acknowledged_ns,"rows":rows,"summary":summary,
            "capture":{"file":capture_name,"rgba_sha256":sha256(&pixels)?,"non_background_pixels":non_background},
            "gpu_timing_report":app.world().get_resource::<GaussianGpuTimings>().and_then(GaussianGpuTimings::report)}));
    }
    let final_commit = command("git", &git_args);
    let final_dirty = command("git", &dirty_args).map(|s| !s.is_empty());
    publication.runtime_dirty = if initial_dirty == Some(false)
        && final_dirty == Some(false)
        && initial_commit == final_commit
    {
        Some(false)
    } else {
        Some(true)
    };
    let publishable = validation::publication_eligible(&publication);
    if !publishable {
        bmf.clear();
    }
    let report = json!({"schema_version":1,"protocol":"dimensify-native-offscreen-v1",
        "timestamp_unix_seconds":SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs(),
        "classification":if options.smoke {"smoke_not_performance"} else if cfg!(debug_assertions) {"debug_not_performance"} else if publishable {"hardware_performance"} else {"diagnostic_not_for_publication"},
        "publication_eligible":publishable,"publication_exclusions":validation::publication_exclusions(&publication),
        "source":{"name":name,"sha256":hash,"bytes":bytes.len(),"generated":options.input.is_none(),"count":count,"storage_rows":storage_rows,
            "source_prepare_ns":source_prepare_ns,"decode_ns":decode_ns,"bounds":[decoded.bounds.0.to_array(),decoded.bounds.1.to_array()]},
        "environment":{"git_commit":initial_commit,"git_dirty":initial_dirty,"git_commit_after":final_commit,"git_dirty_after":final_dirty,
            "build_commit":option_env!("DIMENSIFY_PERF_BUILD_COMMIT"),"build_dirty":option_env!("DIMENSIFY_PERF_BUILD_DIRTY"),
            "build_profile":option_env!("DIMENSIFY_PERF_BUILD_PROFILE"),"build_opt_level":option_env!("DIMENSIFY_PERF_BUILD_OPT_LEVEL"),
            "build_rustc":option_env!("DIMENSIFY_PERF_BUILD_RUSTC"),"executable_sha256":executable_sha256,
            "os_family":std::env::consts::OS,"os":command("uname",&["-sr"]),"architecture":std::env::consts::ARCH,
            "cpu":command("sysctl",&["-n","machdep.cpu.brand_string"]).or_else(||fs::read_to_string("/proc/cpuinfo").ok().and_then(|s|s.lines().find(|s|s.starts_with("model name")).map(str::to_owned))),
            "adapter":{"name":adapter.name,"device_type":device_type,"backend":format!("{:?}",adapter.backend),"vendor":adapter.vendor,"device":adapter.device,"driver":adapter.driver,"driver_info":adapter.driver_info},
            "device_features":features,"device_limits":limits,"debug_assertions":cfg!(debug_assertions),"renderer_setup_ns":setup_ns},
        "render":{"physical_resolution":[options.width,options.height],"target_format":"Rgba8UnormSrgb","msaa":false,"sh_compiled_degree":3,
            "camera_protocol":if explicit_camera.is_some() {"explicit-source-camera-v1"} else {"automatic-bounds-fit-v1"},
            "camera_input":explicit_camera.as_ref().map(|(_,value,hash)|json!({"sha256":hash,"pose":value})),
            "settings":effective_settings,"pipelined_rendering":false,"frames_in_flight":1,"display_present":false,"camera_fov_degrees":45,
            "camera_up":explicit_camera.as_ref().map_or(Vec3::Y, |(path,_,_)|path.up).to_array(),"motion_amplitude_degrees":45,"motion_period_frames":options.samples},
        "definitions":{"app_update_wall_ns":"Synchronous App::update wall interval, including main/render scheduling and command submission; may include internal waits, not pure CPU execution.",
            "completed_frame_ns":"App::update plus RenderDevice::poll Wait for the most recent GPU submission. One frame in flight; includes CPU, scheduling and synchronization overhead; not isolated GPU time or display FPS.",
            "gpu_radix":"Optional native timestamp span for radix compute including visibility compaction and excluding clears; never total GPU render time. Delayed readbacks retain originating sample IDs. Enabling this diagnostic adds query/readback work to measured updates and completion.",
            "startup":"Source preparation, production decode and renderer initialization are separately recorded; warmup, capture and hashing are outside steady-state timing.",
            "memory":"No process RSS, driver residency or GPU allocation accounting is provided by this runner.",
            "quality":"Explicit renderer settings and captures are retained; matching settings does not establish image equivalence with another renderer."},
        "gpu_timestamps_requested":options.timestamps,
        "gpu_timing_unavailable_reason":if !options.timestamps {Some("disabled_by_request")} else if app.world().get_resource::<GaussianGpuTimings>().and_then(GaussianGpuTimings::report).is_none_or(|r|r.capability!=bevy_gaussian_splatting::gpu_timing::Capability::TimestampQuery) {Some("actual_device_timestamp_query_unavailable")} else {None},
        "minimum_completed_work_per_case_ns":2_000_000_000_u64,"cases":cases});
    fs::write(
        options.output.join("raw.json"),
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        options.output.join("bmf.json"),
        serde_json::to_vec_pretty(&bmf).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("{}", options.output.join("raw.json").display());
    Ok(())
}
