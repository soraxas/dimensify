use super::*;
#[path = "output.rs"]
mod output;
use bevy::{
    app::PluginsState,
    render::{render_resource::PollType, renderer::RenderDevice},
};
use std::{fs, path::PathBuf, time::Instant};
#[path = "wait.rs"]
mod wait;

fn frame(
    app: &mut App,
    name: &str,
    phase: &str,
    frame: u64,
    deadline: Instant,
) -> Result<(), String> {
    let update_start = Instant::now();
    app.update();
    let update_seconds = update_start.elapsed().as_secs_f64();
    let snapshots = app.world().resource::<GaussianRenderStatus>().snapshots();
    let ready = snapshots.iter().any(|sample| sample.ready);
    let sort_encoded: usize = snapshots.iter().map(|sample| sample.sort_encoded).sum();
    let draw_encoded: usize = snapshots.iter().map(|sample| sample.draw_encoded).sum();
    if frame <= 3 || frame.is_multiple_of(30) || ready {
        eprintln!(
            "Visual {name} {phase} frame={frame}: update={update_seconds:.3}s ready={ready} sort_encoded={sort_encoded} draw_encoded={draw_encoded}"
        );
    }
    let wait_start = Instant::now();
    let result = wait::wait(
        deadline.saturating_duration_since(wait_start),
        || wait_start.elapsed(),
        |timeout| {
            app.world()
                .resource::<RenderDevice>()
                .poll(PollType::Wait {
                    submission_index: None,
                    timeout: Some(timeout),
                })
                .map(|_| ())
                .map_err(|error| match error {
                    wgpu::PollError::Timeout => wait::WaitError::Timeout,
                    other => wait::WaitError::Other(other.to_string()),
                })
        },
        |elapsed| {
            eprintln!(
                "Visual {name} {phase} frame={frame}: GPU fence pending {:.1}s; ready={ready} sort_encoded={sort_encoded} draw_encoded={draw_encoded}",
                elapsed.as_secs_f64()
            )
        },
    );
    match result {
        Ok(elapsed) => {
            if elapsed >= Duration::from_secs(1) {
                eprintln!(
                    "Visual {name} {phase} frame={frame}: GPU fence completed after {:.3}s",
                    elapsed.as_secs_f64()
                );
            }
            Ok(())
        }
        Err(wait::WaitError::Timeout) => Err(format!(
            "{name} {phase} frame={frame}: GPU completion exceeded phase deadline; update={update_seconds:.3}s ready={ready} sort_encoded={sort_encoded} draw_encoded={draw_encoded}"
        )),
        Err(wait::WaitError::Other(error)) => Err(format!(
            "{name} {phase} frame={frame}: GPU completion failed: {error}"
        )),
    }
}
pub fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut output = None;
    let mut lane = if cfg!(target_os = "macos") {
        "native-metal"
    } else if cfg!(target_os = "windows") {
        "native-dx12"
    } else {
        "native-vulkan"
    }
    .to_owned();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--output needs a directory")?,
                ))
            }
            "--lane" => lane = args.next().ok_or("--lane needs a name")?,
            "--help" | "-h" => {
                println!(
                    "Usage: visual --output NEW_DIRECTORY [--lane native-metal|native-vulkan|native-dx12]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    if !["native-metal", "native-vulkan", "native-dx12"].contains(&lane.as_str()) {
        return Err("unsupported native lane".into());
    }
    let output = output.ok_or("--output is required")?;
    // Never replace or bless a previous run.
    output::create_output(&output)?;
    let mut app = App::new();
    plugins(&mut app);
    app.init_resource::<Captured>();
    let deadline = Instant::now() + Duration::from_secs(90);
    while app.plugins_state() != PluginsState::Ready {
        if Instant::now() > deadline {
            return Err("renderer initialization timed out".into());
        }
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let metadata = metadata(app.world(), &lane);
    let backend = metadata["backend"].as_str().unwrap_or("");
    let expected = match lane.as_str() {
        "native-metal" => "Metal",
        "native-dx12" => "Dx12",
        _ => "Vulkan",
    };
    if backend != expected {
        return Err(format!("lane {lane} requires {expected}, got {backend}"));
    }
    let mut captures = Vec::new();
    for name in CASES {
        let (target, camera, cloud) = setup_scene(app.world_mut(), name);
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut stable = 0;
        let mut frame_id = 0;
        eprintln!(
            "Visual {name}: waiting for three completed production sort/draw frames (90s phase budget)"
        );
        loop {
            frame_id += 1;
            frame(&mut app, name, "readiness", frame_id, deadline)?;
            stable = if acknowledged(app.world()) {
                stable + 1
            } else {
                0
            };
            if stable >= 3 {
                break;
            }
            if Instant::now() > deadline {
                return Err(format!("{name}: production sort/draw readiness timed out"));
            }
        }
        request_capture(app.world_mut(), target);
        let deadline = Instant::now() + Duration::from_secs(90);
        let rgba = loop {
            frame_id += 1;
            frame(&mut app, name, "screenshot", frame_id, deadline)?;
            if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
                break pixels(image)?;
            }
            if Instant::now() > deadline {
                return Err(format!("{name}: screenshot completion timed out"));
            }
        };
        expectations::check(name, &rgba).map_err(|e| format!("{name}: {e}"))?;
        image::save_buffer_with_format(
            output.join(format!("{name}.png")),
            &rgba,
            SIZE,
            SIZE,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|e| e.to_string())?;
        captures.push(capture_row(name));
        app.world_mut().despawn(camera);
        app.world_mut().despawn(cloud);
        eprintln!("Captured {name}: completed GPU sort, draw, screenshot and semantic checks");
    }
    fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&json!({
        "schema":"dimensify-visual-captures/1","metadata":metadata,"captures":captures}))
        .unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
