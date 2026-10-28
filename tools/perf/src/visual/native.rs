use super::*;
#[path = "output.rs"]
mod output;
use bevy::{
    app::PluginsState,
    render::{render_resource::PollType, renderer::RenderDevice},
};
use std::{fs, path::PathBuf, time::Instant};
fn frame(app: &mut App) -> Result<(), String> {
    app.update();
    app.world()
        .resource::<RenderDevice>()
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
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
        loop {
            frame(&mut app)?;
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
        let rgba = loop {
            frame(&mut app)?;
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
