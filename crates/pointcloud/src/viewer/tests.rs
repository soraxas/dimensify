//! Windowless captures of the production viewer, including its actual UI.
use super::*;
use bevy::{
    app::PluginsState,
    camera::RenderTarget,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    text::TextLayoutInfo,
    time::TimeUpdateStrategy,
    window::PrimaryWindow,
};
use std::{
    net::TcpListener,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Resource, Default)]
struct Capture(Option<Image>);

#[derive(Resource)]
struct Target(Handle<Image>);

struct PendingServer {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl PendingServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/metadata.json", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut clients = Vec::new();
            while !stopping.load(Ordering::Relaxed) {
                if let Ok((client, _)) = listener.accept() {
                    clients.push(client);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for PendingServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn retarget(
    mut commands: Commands,
    cameras: Query<Entity, With<ViewerCamera>>,
    target: Res<Target>,
) {
    for camera in &cameras {
        commands
            .entity(camera)
            .insert(RenderTarget::Image(target.0.clone().into()));
    }
}

fn keyboard_event(app: &mut App, key_code: KeyCode, state: ButtonState) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world())
        .unwrap();
    app.world_mut().write_message(KeyboardInput {
        key_code,
        logical_key: Key::Character("capture-input".into()),
        state,
        text: None,
        repeat: false,
        window,
    });
    app.update();
    app.update();
}

fn capture(
    app: &mut App,
    cloud: Entity,
    phase: PointcloudPhase,
    size: UVec2,
    name: &str,
    output: &Path,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut settled = 0;
    let mut pending = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "{name}: capture/readiness timed out"
        );
        app.update();
        if app
            .world()
            .get::<PointcloudStatus>(cloud)
            .is_some_and(|s| s.phase == phase && s.active == 0 && s.queued == 0)
        {
            settled += 1;
        } else {
            settled = 0;
        }
        if settled < 12 {
            continue;
        }
        if let Some(image) = app.world_mut().resource_mut::<Capture>().0.take() {
            let image = image.try_into_dynamic().unwrap().into_rgba8();
            assert_eq!((image.width(), image.height()), (size.x, size.y));
            let text_pixels = image
                .enumerate_pixels()
                .filter(|(x, y, p)| {
                    *x >= 12
                        && *x < size.x - 12
                        && *y >= 12
                        && *y < 140
                        && p.0[..3].iter().all(|c| *c > 170)
                })
                .count();
            assert!(
                text_pixels > 100,
                "{name}: missing rendered text ({text_pixels} pixels)"
            );
            let cloud_pixels = image
                .enumerate_pixels()
                .filter(|(_, y, p)| *y > 160 && p.0[2] > 70 && p.0[1] > 40)
                .count();
            if phase == PointcloudPhase::Ready {
                assert!(cloud_pixels > 500, "{name}: missing rendered cloud");
            } else {
                assert_eq!(cloud_pixels, 0, "{name}: stale cloud remains visible");
            }
            let mut query = app
                .world_mut()
                .query_filtered::<(&Text, &ComputedNode, &TextLayoutInfo), With<StatusText>>();
            let (text, node, layout) = query.single(app.world()).unwrap();
            assert!(!layout.glyphs.is_empty(), "{name}: missing laid-out glyphs");
            assert!(
                layout.size.x <= node.size.x + 1.,
                "{name}: text overflows horizontally: {:?} > {:?}",
                layout.size,
                node.size
            );
            assert!(
                node.size.y < size.y as f32 - 24.,
                "{name}: text overflows viewport"
            );
            assert!(text.0.contains("F: frame") && text.0.contains("R: reset"));
            let status = app.world().get::<PointcloudStatus>(cloud).unwrap();
            assert!(
                text.0
                    .starts_with(status.error.as_deref().unwrap_or_else(|| match phase {
                        PointcloudPhase::Ready => "Ready",
                        PointcloudPhase::Metadata => "Metadata",
                        _ => unreachable!(),
                    }))
            );
            std::fs::write(output.join(format!("{name}.txt")), &text.0).unwrap();
            let mut ppm = format!("P6\n{} {}\n255\n", size.x, size.y).into_bytes();
            for pixel in image.pixels() {
                ppm.extend_from_slice(&pixel.0[..3]);
            }
            std::fs::write(output.join(format!("{name}.ppm")), ppm).unwrap();
            println!(
                "{name}: {text_pixels} text pixels, {cloud_pixels} cloud pixels, text bounds {:?}",
                layout.size
            );
            return;
        }
        if !pending {
            pending = true;
            let target = app.world().resource::<Target>().0.clone();
            app.world_mut().spawn(Screenshot::image(target)).observe(
                |event: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
                    capture.0 = Some(event.image.clone())
                },
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires native GPU; captures the production point-cloud viewer UI"]
fn viewer_images_and_keyboard_work_offscreen() {
    let temporary = tempfile::tempdir().unwrap();
    let output = std::env::var_os("DIMENSIFY_POINTCLOUD_VIEWER_CAPTURE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("captures"));
    std::fs::create_dir(&output).expect("capture directory must be new");
    let server = PendingServer::start();
    for (label, size) in [
        ("desktop", UVec2::new(1000, 760)),
        ("narrow", UVec2::new(360, 640)),
    ] {
        let source = LoadPointcloud::mesh(fixture_mesh());
        let PointcloudSource::Mesh(mesh) = &source.source else {
            unreachable!()
        };
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            unreachable!()
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            unreachable!()
        };
        std::fs::write(
            output.join("scene.json"),
            serde_json::to_vec(&serde_json::json!({
                "positions": positions, "colors": colors,
                "states": ["ready", "invalid-http-source", "pending-http"],
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            output.join(format!("{label}-settings.json")),
            serde_json::to_vec(&serde_json::json!({
                "viewport": [size.x, size.y], "timeStep": "1/60", "minimumSettledFrames": 12,
                "splats": source.settings.splats, "unlit": source.settings.unlit,
                "sourceUp": format!("{:?}", source.up),
                "sourceTransform": source.transform.to_matrix().to_cols_array(),
            }))
            .unwrap(),
        )
        .unwrap();
        let mut plugins = DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    resolution: (size.x, size.y).into(),
                    ..default()
                }),
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>();
        if label == "narrow" {
            // Bevy's logger is process-global; the desktop app installed it.
            plugins = plugins.disable::<bevy::log::LogPlugin>();
        }
        let mut app = viewer_app(source, plugins);
        app.init_resource::<Capture>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1. / 60.,
            )))
            .add_systems(Startup, retarget.after(setup));
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.plugins_state() != PluginsState::Ready {
            assert!(Instant::now() < deadline, "GPU initialization timed out");
            bevy::tasks::tick_global_task_pools_on_main_thread();
            std::thread::yield_now();
        }
        app.finish();
        app.cleanup();
        let adapter = app
            .world()
            .resource::<bevy::render::renderer::RenderAdapterInfo>();
        std::fs::write(
            output.join(format!("{label}-adapter.txt")),
            format!(
                "{}\n{:?}\n{}\n{}\n",
                adapter.name, adapter.backend, adapter.driver, adapter.driver_info
            ),
        )
        .unwrap();
        let target =
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    size.x,
                    size.y,
                    TextureFormat::Rgba8UnormSrgb,
                    None,
                ));
        app.insert_resource(Target(target));
        app.update();
        let cloud = app
            .world_mut()
            .query_filtered::<Entity, With<PointcloudStatus>>()
            .single(app.world())
            .unwrap();
        capture(
            &mut app,
            cloud,
            PointcloudPhase::Ready,
            size,
            &format!("{label}-ready"),
            &output,
        );
        keyboard_event(&mut app, KeyCode::KeyM, ButtonState::Pressed);
        assert_eq!(
            *app.world().resource::<NavigationMode>(),
            NavigationMode::Fly
        );
        keyboard_event(&mut app, KeyCode::KeyM, ButtonState::Released);
        keyboard_event(&mut app, KeyCode::KeyM, ButtonState::Pressed);
        assert_eq!(
            *app.world().resource::<NavigationMode>(),
            NavigationMode::Orbit
        );
        keyboard_event(&mut app, KeyCode::KeyM, ButtonState::Released);
        let generation = app
            .world()
            .resource::<dimensify_camera::viewer::CameraPlaybackStatus>()
            .generation;
        keyboard_event(&mut app, KeyCode::KeyF, ButtonState::Pressed);
        assert!(
            app.world()
                .resource::<dimensify_camera::viewer::CameraPlaybackStatus>()
                .generation
                > generation
        );
        keyboard_event(&mut app, KeyCode::KeyF, ButtonState::Released);
        keyboard_event(&mut app, KeyCode::KeyR, ButtonState::Pressed);
        let home = app
            .world()
            .resource::<NavigationConfig>()
            .home
            .to_transform();
        let transform = app
            .world_mut()
            .query_filtered::<&Transform, With<ViewerCamera>>()
            .single(app.world())
            .unwrap();
        assert!(transform.translation.abs_diff_eq(home.translation, 1e-4));
        keyboard_event(&mut app, KeyCode::KeyR, ButtonState::Released);
        app.world_mut()
            .entity_mut(cloud)
            .insert(LoadPointcloud::potree("invalid-source"));
        capture(
            &mut app,
            cloud,
            PointcloudPhase::Failed,
            size,
            &format!("{label}-failed"),
            &output,
        );
        app.world_mut()
            .entity_mut(cloud)
            .insert(LoadPointcloud::potree(&server.url));
        capture(
            &mut app,
            cloud,
            PointcloudPhase::Metadata,
            size,
            &format!("{label}-loading"),
            &output,
        );
    }
}
