//! Native replay protocol for the production streamed packed renderer.
#[path = "../gpu/camera.rs"]
#[allow(
    dead_code,
    reason = "The LOD replay uses camera parsing and supplies its own trajectory."
)]
mod camera;
mod memory;
mod options;
#[path = "../gpu/passes.rs"]
mod passes;
#[path = "../gpu/stats.rs"]
mod stats;
mod support;
#[path = "../gpu/validation.rs"]
mod validation;

use bevy::{
    prelude::*,
    render::renderer::{RenderAdapterInfo, RenderDevice},
};
use bevy_gaussian_splatting::{
    diagnostics::{GaussianDiagnosticSample, GaussianRenderStatus},
    gpu_timing::{GaussianGpuTimings, GaussianTimingView},
};
use dimensify_splat::{
    assets::{AssetLocation, AssetResolver},
    diagnostics::{
        quality::{SogDiagnosticSettings, SogLeafSnapshot},
        sog_cache::SogCacheTrace,
        sog_cpu::SogCpuTimings,
    },
    renderer::{
        gpu::GpuBudget,
        telemetry::{SogGpuTelemetry, SogGpuTimingIdentity},
    },
    runtime::SogProjectionCache,
    streaming::{
        SogLodSettings,
        runtime::{SogStream, StreamingCamera},
    },
};
use options::Options;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HELP: &str = "Usage: lod --manifest HIERARCHY.json --output NEW_DIRECTORY [--preset default|constrained] [--transparency sorted|stochastic] [--camera SOURCE_CAMERA.json] [--gpu-timestamps] [--width 1280] [--height 720] [--samples 240] [--warmup 32] [--smoke]\nReplays static, moving and near-far-return phases through production SogStream loading, scheduling and packed projection/rendering. Writes raw.json, bmf.json and nonblank PPM captures. Native completion waits are not display frame time. Publication requires a clean matching optimized hardware build. Assets are read from the manifest directory; no network fetch is performed.";

pub fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return;
    }
    if let Err(error) = Options::parse(args).and_then(run) {
        eprintln!("LOD benchmark failed: {error}");
        std::process::exit(1);
    }
}

fn status(app: &App, root: Entity) -> Result<Value, String> {
    let stream = app
        .world()
        .get::<SogStream>(root)
        .ok_or("stream disappeared")?;
    if let Some(error) = stream.terminal_error() {
        return Err(format!("stream failed: {error}"));
    }
    let s = stream.status();
    Ok(
        json!({"evaluated":s.evaluated,"requested":s.requested,"effective":s.effective,
        "selected":s.selected,"displayed":s.displayed,"source_limited":s.source_limited,
        "selection_limited":s.selection_limited,"pending_downloads":s.pending_downloads,
        "pending_generation":s.pending_generation,"pending_work":s.pending_work,
        "limiting_reason":s.limiting_reason,"cache_ready_bytes":s.cache_ready_bytes,"cache_reserved_bytes":s.cache_reserved_bytes}),
    )
}

fn frame(app: &mut App, view: Entity, root: Entity, id: u64) -> Result<Value, String> {
    app.world_mut()
        .entity_mut(view)
        .insert(GaussianDiagnosticSample(id));
    app.world_mut().insert_resource(SogGpuTimingIdentity {
        scene: 1,
        selection: id,
    });
    let start = Instant::now();
    app.update();
    let update_ns = start.elapsed().as_nanos();
    support::wait_for_gpu(app)?;
    let completed_ns = start.elapsed().as_nanos();
    let acks = app.world().resource::<GaussianRenderStatus>().snapshots();
    let acknowledged = acks
        .iter()
        .any(|s| s.sample_id == id && s.ready && s.draw_encoded > 0 && s.sort_encoded > 0);
    let stochastic = app
        .world()
        .resource::<dimensify_splat::runtime::SplatTransparency>()
        .0
        == bevy_gaussian_splatting::GaussianTransparency::Stochastic;
    let projected_render_acknowledged = acks.iter().any(|s| {
        s.sample_id == id
            && s.cloud_count > 0
            && if stochastic {
                s.projected_stochastic_ready
            } else {
                s.projected_ready
            }
    });
    let projected_acknowledged = acks.iter().any(|s| {
        s.sample_id == id
            && s.projected_ready
            && s.projected_draw_encoded == s.cloud_count
            && s.cloud_count > 0
    });
    let pose = app
        .world()
        .get::<Transform>(view)
        .ok_or("camera disappeared")?;
    Ok(
        json!({"sample_id":id,"app_update_wall_ns":update_ns,"completed_frame_ns":completed_ns,
        "acknowledged_sort_and_draw":acknowledged,
        "acknowledged_projected_render":projected_render_acknowledged,
        "acknowledged_projected_sort_and_draw":projected_acknowledged,"render_status":acks.iter().filter(|s|s.sample_id==id).map(|s|json!({"cloud_count":s.cloud_count,"sort_encoded":s.sort_encoded,"draw_encoded":s.draw_encoded,"projected_draw_encoded":s.projected_draw_encoded,"ready":s.ready,"projected_ready":s.projected_ready,"stochastic_draw_encoded":s.stochastic_draw_encoded,"stochastic_ready":s.stochastic_ready,"projected_stochastic_ready":s.projected_stochastic_ready})).collect::<Vec<_>>(),
        "camera_position":pose.translation.to_array(),"camera_rotation_xyzw":pose.rotation.to_array(),
        "stream":status(app,root)?,"leaf_snapshot":app.world().resource::<SogLeafSnapshot>(),
        "cpu_timing_report":app.world().resource::<SogCpuTimings>().report()}),
    )
}

fn collect(app: &App, ids: &BTreeSet<u64>, samples: &mut passes::PassSamples) -> Vec<Value> {
    let mut reports: Vec<Value> = app
        .world()
        .get_resource::<GaussianGpuTimings>()
        .and_then(GaussianGpuTimings::pass_reports)
        .unwrap_or_default()
        .into_iter()
        .map(|r| serde_json::to_value(r).expect("serializable timestamp report"))
        .collect();
    if let Some(report) = app
        .world()
        .get_resource::<GaussianGpuTimings>()
        .and_then(GaussianGpuTimings::report)
    {
        reports.push(serde_json::to_value(report).expect("serializable radix report"));
    }
    for report in &reports {
        samples.collect(report, ids);
    }
    reports
}

fn pose(path: &camera::CameraPath, phase: &str, frame: usize, period: usize) -> Transform {
    let fraction = frame as f32 / period as f32;
    let orbit = if phase == "moving" {
        0.65 * (fraction * std::f32::consts::TAU).sin()
    } else {
        0.0
    };
    // Same start/end pose; the middle visits far then near, exercising release,
    // source admission, selected working slots and return-to-view reuse.
    let scale = if phase == "near-far-return" {
        let t = fraction.fract() * 4.0;
        match t {
            t if t < 1.0 => 1.0 + 3.0 * t,
            t if t < 2.0 => 4.0 - 3.65 * (t - 1.0),
            t if t < 3.0 => 0.35,
            _ => 0.35 + 0.65 * (t - 3.0),
        }
    } else {
        1.0
    };
    let offset = Quat::from_axis_angle(path.up, orbit) * path.offset * scale;
    Transform::from_translation(path.target + offset).looking_at(path.target, path.up)
}

fn run(options: Options) -> Result<(), String> {
    validation::create_output(&options.output)?;
    let mut process = memory::ProcessUsage::new();
    process.checkpoint("before_input");
    let prepare = Instant::now();
    let manifest =
        fs::canonicalize(&options.manifest).map_err(|e| format!("manifest path: {e}"))?;
    let bytes = fs::read(&manifest).map_err(|e| format!("read manifest: {e}"))?;
    let hash = support::sha256(&bytes)?;
    let root_directory = manifest
        .parent()
        .ok_or("manifest has no parent")?
        .to_path_buf();
    let resolver = AssetResolver::new(move |reference| {
        let relative = reference
            .strip_prefix("https://dimensify.invalid/fixture/")
            .ok_or("unexpected source reference")?;
        let path = fs::canonicalize(root_directory.join(relative))
            .map_err(|e| format!("resolve {relative}: {e}"))?;
        if !path.starts_with(&root_directory) {
            return Err("source escapes fixture directory".into());
        }
        fs::read(path)
            .map(|bytes| AssetLocation::Bytes(bytes.into()))
            .map_err(|e| format!("read {relative}: {e}"))
    });
    let stream = SogStream::parse(
        &bytes,
        "https://dimensify.invalid/fixture/lod-meta.json",
        "native LOD fixture".into(),
        "native-lod".into(),
    )?
    .with_resolver(resolver);
    let extent = stream.fit_extent();
    let bounds = [
        (extent.center - extent.half_extents).to_array(),
        (extent.center + extent.half_extents).to_array(),
    ];
    let (path, camera_input) = if let Some(file) = &options.camera {
        let bytes = fs::read(file).map_err(|e| format!("read camera: {e}"))?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        (
            camera::CameraPath::from_json(&value)?,
            json!({"sha256":support::sha256(&bytes)?,"pose":value}),
        )
    } else {
        let distance = extent.half_extents.length().max(0.1) / 22.5_f32.to_radians().sin() * 1.12;
        (
            camera::CameraPath {
                target: extent.center,
                offset: Vec3::new(0.0, 0.0, -distance),
                up: Vec3::NEG_Y,
            },
            Value::Null,
        )
    };
    let source_prepare_ns = prepare.elapsed().as_nanos();
    let git_args = ["-C", env!("CARGO_MANIFEST_DIR"), "rev-parse", "HEAD"];
    let dirty_args = ["-C", env!("CARGO_MANIFEST_DIR"), "status", "--porcelain"];
    let initial_commit = support::command("git", &git_args);
    let initial_dirty = support::command("git", &dirty_args).map(|s| !s.is_empty());
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let executable_sha256 = support::sha256(&fs::read(executable).map_err(|e| e.to_string())?)?;
    let setup_start = Instant::now();
    let (mut app, target, view) = support::setup(&options)?;
    let setup_ns = setup_start.elapsed().as_nanos();
    app.world_mut().entity_mut(view).insert(StreamingCamera);
    let mut lod = SogLodSettings::default();
    let mut budget = GpuBudget::default();
    if options.preset == "constrained" {
        lod.splat_budget = 125_000;
        lod.cache_bytes = 32 * 1024 * 1024;
        budget.source_bytes = 32 * 1024 * 1024;
        budget.selected_bytes_per_scene = 64 * 1024 * 1024;
    }
    let settings = json!({"lod_policy":format!("{:?}",lod.lod_policy),"target_pixels":lod.target_pixels,
        "splat_budget":lod.splat_budget,"cache_bytes":lod.cache_bytes,"concurrent_chunks":lod.concurrent_chunks,
        "visibility":format!("{:?}",lod.effective_visibility()),"gpu_source_budget_bytes":budget.source_bytes,
        "gpu_selected_budget_bytes_per_scene":budget.selected_bytes_per_scene});
    app.insert_resource(dimensify_splat::runtime::SplatTransparency(
        options.transparency,
    ))
    .add_systems(
        PostUpdate,
        dimensify_splat::runtime::apply_transparency
            .after(dimensify_splat::streaming::runtime::update_sog_streams),
    )
    .insert_resource(lod)
    .insert_resource(budget)
    .insert_resource(SogProjectionCache(true))
    .insert_resource(SogDiagnosticSettings { enabled: true })
    .insert_resource(SogCpuTimings::enabled())
    .insert_resource(SogCacheTrace::enabled());
    let root = app
        .world_mut()
        .spawn((stream, Transform::IDENTITY, Visibility::Inherited))
        .id();
    let adapter = app.world().resource::<RenderAdapterInfo>().0.clone();
    let device_type = format!("{:?}", adapter.device_type);
    let backend = format!("{:?}", adapter.backend);
    let device_memory_probe = dimensify_perf::device_memory::validate_active_device(
        app.world().resource::<RenderDevice>(),
        &adapter.name,
        &backend,
    );
    let mut device_memory_checkpoints = vec![json!({"phase":"after_renderer_setup",
        "measurement":dimensify_perf::device_memory::snapshot(&adapter.name,&backend)})];
    validation::validate_adapter(&device_type, &adapter.name, options.smoke)?;
    let features = format!("{:?}", app.world().resource::<RenderDevice>().features());
    let limits = format!("{:?}", app.world().resource::<RenderDevice>().limits());
    process.checkpoint("after_renderer_setup");
    let mut gpu_inventory = Vec::new();
    let mut next_id = 0_u64;
    let mut cases = Vec::new();
    let mut bmf = serde_json::Map::new();
    for phase in ["static", "moving", "near-far-return"] {
        eprintln!("LOD {phase}: waiting for production packed draw");
        if options.timestamps {
            app.world_mut().entity_mut(view).insert(GaussianTimingView);
        }
        let warm_start = Instant::now();
        let mut stable = 0;
        let mut warm_frames = 0;
        let mut first_ready_ns = None;
        loop {
            *app.world_mut().get_mut::<Transform>(view).unwrap() =
                pose(&path, phase, 0, options.samples);
            next_id += 1;
            let row = frame(&mut app, view, root, next_id)?;
            warm_frames += 1;
            if row["acknowledged_sort_and_draw"] == true
                && row["acknowledged_projected_render"] != true
            {
                return Err(format!(
                    "{phase}: renderer fell back from compute-projected draw: {row}"
                ));
            }
            if row["acknowledged_projected_render"] == true {
                first_ready_ns.get_or_insert_with(|| warm_start.elapsed().as_nanos());
                if row["stream"]["pending_generation"] == false
                    && row["stream"]["pending_downloads"] == 0
                    && row["stream"]["pending_work"] == false
                {
                    stable += 1;
                } else {
                    stable = 0;
                }
            }
            if stable >= options.warmup {
                break;
            }
            if warm_start.elapsed() > Duration::from_secs(120) {
                return Err(format!("{phase}: packed loading/warmup timed out; {row}"));
            }
            std::thread::yield_now();
        }
        let warm_ns = warm_start.elapsed().as_nanos();
        process.checkpoint(&format!("before_view:{phase}"));
        device_memory_checkpoints.push(json!({"phase":format!("before_view:{phase}"),
            "measurement":dimensify_perf::device_memory::snapshot(&adapter.name,&backend)}));
        gpu_inventory.push(json!({"phase":format!("before_view:{phase}"),"sample_id":next_id,"telemetry":app.world().resource::<SogGpuTelemetry>().report()}));
        let mut rows = Vec::new();
        let mut ids = BTreeSet::new();
        let mut pass_samples = passes::PassSamples::default();
        let mut completed_ns = 0.0;
        let mut index = 0;
        while index < options.samples
            || completed_ns < 2_000_000_000.0
            || !index.is_multiple_of(options.samples)
        {
            *app.world_mut().get_mut::<Transform>(view).unwrap() =
                pose(&path, phase, index % options.samples, options.samples);
            next_id += 1;
            ids.insert(next_id);
            let row = frame(&mut app, view, root, next_id)?;
            if row["acknowledged_projected_render"] != true {
                return Err(format!(
                    "{phase}: measured frame lacks real projected sort/draw: {row}"
                ));
            }
            completed_ns += row["completed_frame_ns"].as_f64().unwrap();
            rows.push(row);
            index += 1;
            collect(&app, &ids, &mut pass_samples);
        }
        // The telemetry plugin tags every originating frame. Drain under an
        // unmeasured identity so its own work never aliases the last sample.
        app.world_mut().insert_resource(SogGpuTimingIdentity {
            scene: 1,
            selection: 0,
        });
        app.world_mut()
            .entity_mut(view)
            .remove::<GaussianTimingView>();
        for _ in 0..3 {
            app.update();
            support::wait_for_gpu(&app)?;
            collect(&app, &ids, &mut pass_samples);
        }
        let reports = collect(&app, &ids, &mut pass_samples);
        for row in &mut rows {
            row["gpu_pass_samples"] =
                json!(pass_samples.for_selection(row["sample_id"].as_u64().unwrap()));
        }
        let completed: Vec<_> = rows
            .iter()
            .map(|r| r["completed_frame_ns"].as_f64().unwrap())
            .collect();
        let update: Vec<_> = rows
            .iter()
            .map(|r| r["app_update_wall_ns"].as_f64().unwrap())
            .collect();
        let durations = pass_samples.durations_ns();
        let mut summary = json!({"completed_frame":stats::distribution(&completed)?,"app_update_wall":stats::distribution(&update)?,"gpu_passes":{}});
        for (scope, values) in &durations {
            summary["gpu_passes"][scope] = stats::distribution(values)?;
        }
        let coverage = pass_samples.coverage(&reports, rows.len());
        process.checkpoint(&format!("after_view:{phase}"));
        gpu_inventory.push(json!({"phase":format!("after_view:{phase}"),"sample_id":next_id,"telemetry":app.world().resource::<SogGpuTelemetry>().report()}));
        device_memory_checkpoints.push(json!({"phase":format!("after_view:{phase}"),
            "measurement":dimensify_perf::device_memory::snapshot(&adapter.name,&backend)}));
        let pixels = support::capture(&mut app, &target)?;
        let non_background = validation::validate_pixels(&pixels, options.width, options.height)?;
        let capture_name = format!("{phase}.ppm");
        support::write_ppm(
            &options.output.join(&capture_name),
            &pixels,
            options.width,
            options.height,
        )?;
        for (metric, values) in [
            ("completed_frame", &completed),
            ("app_update_wall", &update),
        ]
        .into_iter()
        .chain(durations.iter().map(|(s, v)| (s.as_str(), v)))
        {
            if values.len() != rows.len() || values.iter().any(|v| *v <= 0.0) {
                continue;
            }
            bmf.insert(
                format!(
                    "{}/{phase}/{metric}",
                    bmf_workload(&hash, &options, &camera_input)
                ),
                stats::bmf_metric(values)?,
            );
        }
        eprintln!(
            "LOD {phase}: {} completed frames; {} non-background pixels",
            rows.len(),
            non_background
        );
        cases.push(json!({"name":phase,"warmup_frames":warm_frames,"warmup_ns":warm_ns,
            "first_acknowledged_completed_draw_ns":first_ready_ns,"rows":rows,"summary":summary,
            "capture":{"file":capture_name,"rgba_sha256":support::sha256(&pixels)?,"non_background_pixels":non_background},
            "gpu_pass_reports":reports,"gpu_pass_coverage":coverage,
            "cache_trace":app.world().resource::<SogCacheTrace>().report()}));
    }
    let final_commit = support::command("git", &git_args);
    let final_dirty = support::command("git", &dirty_args).map(|s| !s.is_empty());
    let publication = validation::PublicationEvidence {
        smoke: options.smoke,
        debug: cfg!(debug_assertions),
        hardware: validation::hardware_adapter(&device_type, &adapter.name),
        build_commit: option_env!("DIMENSIFY_PERF_BUILD_COMMIT"),
        build_dirty: option_env!("DIMENSIFY_PERF_BUILD_DIRTY").and_then(|s| s.parse().ok()),
        build_opt_level: option_env!("DIMENSIFY_PERF_BUILD_OPT_LEVEL"),
        runtime_commit: initial_commit.as_deref(),
        runtime_dirty: Some(
            initial_dirty != Some(false)
                || final_dirty != Some(false)
                || initial_commit != final_commit,
        ),
    };
    let publishable = validation::publication_eligible(&publication);
    if !publishable {
        bmf.clear();
    }
    let report = json!({"schema_version":1,"protocol":if options.transparency == bevy_gaussian_splatting::GaussianTransparency::Sorted { "dimensify-native-lod-offscreen-v1" } else { "dimensify-native-lod-stochastic-offscreen-v1" },
        "timestamp_unix_seconds":SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs(),
        "classification":if options.smoke {"smoke_not_performance"} else if publishable {"hardware_performance"} else {"diagnostic_not_for_publication"},
        "publication_eligible":publishable,"publication_exclusions":validation::publication_exclusions(&publication),
        "source":{"name":manifest.file_name().and_then(|s|s.to_str()),"sha256":hash,"manifest_sha256":hash,"manifest":manifest,"bounds":bounds,"bytes":bytes.len(),"source_prepare_ns":source_prepare_ns,"generated":false,"resolver":"local_disk_bytes_production_loader"},
        "environment":{"git_commit":initial_commit,"git_dirty":initial_dirty,"git_commit_after":final_commit,"git_dirty_after":final_dirty,
            "build_commit":option_env!("DIMENSIFY_PERF_BUILD_COMMIT"),"build_dirty":option_env!("DIMENSIFY_PERF_BUILD_DIRTY"),
            "build_profile":option_env!("DIMENSIFY_PERF_BUILD_PROFILE"),"build_opt_level":option_env!("DIMENSIFY_PERF_BUILD_OPT_LEVEL"),
            "build_rustc":option_env!("DIMENSIFY_PERF_BUILD_RUSTC"),"executable_sha256":executable_sha256,
            "os_family":std::env::consts::OS,"os":support::command("uname",&["-sr"]),"architecture":std::env::consts::ARCH,
            "cpu":support::command("sysctl",&["-n","machdep.cpu.brand_string"]).or_else(||fs::read_to_string("/proc/cpuinfo").ok().and_then(|s|s.lines().find(|s|s.starts_with("model name")).map(str::to_owned))),
            "adapter":{"name":adapter.name,"device_type":device_type,"backend":format!("{:?}",adapter.backend),"vendor":adapter.vendor,"device":adapter.device,"driver":adapter.driver,"driver_info":adapter.driver_info},
            "device_features":features,"device_limits":limits,"debug_assertions":cfg!(debug_assertions),"renderer_setup_ns":setup_ns},
        "render":{"backend":"packed","projection_cache":true,"transparency":options.transparency.name(),"stochastic_seed":0,"stochastic_sample":0,
            "resource_instrumentation":{"native_device_resource_counter":"metal-currentAllocatedSize-verified-1MiB-probe-v1","probe_outside_timed_updates":true},"physical_resolution":[options.width,options.height],"preset":options.preset,"settings":settings,
            "camera_protocol":if camera_input.is_null(){"manifest-bounds-negative-y-v1"}else{"explicit-source-camera-v1"},"camera_input":camera_input,
            "camera_target":path.target.to_array(),"camera_offset":path.offset.to_array(),"camera_up":path.up.to_array(),"camera_fov_degrees":45,
            "motion_period_frames":options.samples,"sh_compiled_degree":3,"target_format":"Rgba8UnormSrgb","internal_color_format":"Rgba16Float","encoded_compositing":true,"hdr":true,"msaa":false,"pipelined_rendering":false,"frames_in_flight":1,"display_present":false},
        "resource_usage":process.report(gpu_inventory),
        "device_resource_usage":{"allocation_probe":device_memory_probe,"checkpoints":device_memory_checkpoints},
        "gpu_timestamps_requested":options.timestamps,
        "gpu_timing_report":app.world().get_resource::<GaussianGpuTimings>().and_then(GaussianGpuTimings::report),
        "minimum_completed_work_per_case_ns":2_000_000_000_u64,"cases":cases,
        "definitions":{"completed_frame_ns":"App::update plus GPU poll Wait, one frame in flight; includes CPU scheduling/synchronization; neither isolated GPU time nor display FPS",
            "gpu_passes":"Actual separately timed eligible projection/radix/raster/compositing segments retain scene/frame/selection IDs; coverage excludes missing or invalid segments. Scopes are never summed into total GPU frame time",
            "cpu_timing_report":"Cumulative attempted production CPU stage wall intervals, not thread CPU time; per-row reports may include asynchronous decode",
            "quality":"Displayed is renderer-ready logical splat count; captures prove nonblank rendering, not visual equivalence",
            "resources":"RSS checkpoints are outside timed updates. Buffer inventories retain actual IDs/capacities/lifetimes; portable total driver residency remains unavailable"}});
    write_json(&options.output.join("raw.json"), &report)?;
    write_json(&options.output.join("bmf.json"), &json!(bmf))?;
    println!("{}", options.output.join("raw.json").display());
    Ok(())
}
fn bmf_workload(hash: &str, options: &Options, camera_input: &Value) -> String {
    let camera = camera_input["sha256"].as_str().map_or_else(
        || "manifest-bounds-negative-y-v1".to_owned(),
        |hash| format!("explicit-source-camera-v1-{}", &hash[..12]),
    );
    format!(
        "splat/native-lod/{}/{}x{}/{}/{camera}/timestamps-{}/period-{}",
        &hash[..12],
        options.width,
        options.height,
        if options.transparency == bevy_gaussian_splatting::GaussianTransparency::Sorted {
            options.preset.clone()
        } else {
            format!("{}-stochastic", options.preset)
        },
        if options.timestamps { "on" } else { "off" },
        options.samples
    )
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_bmf_workloads_distinguish_camera_and_timestamp_settings() {
        let mut options = Options::parse(vec!["--manifest".into(), "fixture.json".into()]).unwrap();
        let hash = "a".repeat(64);
        let default = bmf_workload(&hash, &options, &Value::Null);
        let camera = json!({"sha256":"b".repeat(64)});
        let explicit = bmf_workload(&hash, &options, &camera);
        assert_ne!(default, explicit);
        assert_ne!(
            explicit,
            bmf_workload(&hash, &options, &json!({"sha256":"c".repeat(64)}))
        );
        options.timestamps = true;
        assert_ne!(default, bmf_workload(&hash, &options, &Value::Null));
    }
    #[test]
    fn replay_visits_near_and_far_then_returns_to_same_authored_camera() {
        let path = camera::CameraPath {
            target: Vec3::new(1.0, 2.0, 3.0),
            offset: Vec3::new(0.0, 0.0, -4.0),
            up: Vec3::NEG_Y,
        };
        let distance = |frame| {
            pose(&path, "near-far-return", frame, 240)
                .translation
                .distance(path.target)
        };
        assert!((distance(0) - 4.0).abs() < 1e-5);
        assert!((distance(60) - 16.0).abs() < 1e-5);
        assert!((distance(120) - 1.4).abs() < 1e-5);
        assert!((distance(240) - 4.0).abs() < 1e-5);
        let static_pose = pose(&path, "static", 0, 240);
        assert_eq!(static_pose, pose(&path, "static", 110, 240));
        assert!(
            pose(&path, "moving", 60, 240)
                .translation
                .distance(static_pose.translation)
                > 1.0
        );
    }
}
