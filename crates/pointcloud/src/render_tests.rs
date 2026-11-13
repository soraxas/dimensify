//! Explicit offscreen acceptance; excluded from ordinary machines without a GPU.
use super::*;
use bevy::{
    app::PluginsState,
    camera::RenderTarget,
    core_pipeline::tonemapping::Tonemapping,
    prelude::*,
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        render_resource::TextureFormat,
        view::{
            NoIndirectDrawing,
            screenshot::{Screenshot, ScreenshotCaptured},
        },
    },
};
use std::time::{Duration, Instant};

#[derive(Resource, Default)]
struct Captured(Option<Image>);

#[test]
#[ignore = "requires a native GPU; run explicitly for point-cloud renderer changes"]
fn fixture_loads_and_renders_offscreen() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>(),
    )
    .add_plugins(PointcloudPlugin::default())
    .init_resource::<Captured>()
    .insert_resource(ClearColor(Color::BLACK));
    let deadline = Instant::now() + Duration::from_secs(60);
    while app.plugins_state() != PluginsState::Ready {
        assert!(Instant::now() < deadline, "GPU initialization timed out");
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            256,
            256,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    app.world_mut().spawn((
        Camera3d::default(),
        RenderTarget::Image(target.clone().into()),
        Transform::from_xyz(0., 0., 6.).looking_at(Vec3::ZERO, Vec3::Y),
        Msaa::Off,
        Tonemapping::None,
        NoIndirectDrawing,
        renderer::PointCloudVisibilitySettings {
            point_budget: Some(10_000),
            min_radius: Some(0.),
            ..default()
        },
    ));
    let mut mesh = Sphere::new(1.5).mesh().uv(32, 16);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[0.2_f32, 0.7, 0.9, 1.]; mesh.count_vertices()],
    );
    let cloud = app.world_mut().spawn(LoadPointcloud::mesh(mesh)).id();
    let mut capture_pending = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "No colored points rendered: {:?}",
            app.world().get::<PointcloudStatus>(cloud)
        );
        app.update();
        if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
            capture_pending = false;
            let image = image.try_into_dynamic().unwrap().into_rgba8();
            let colored = image
                .pixels()
                .filter(|p| p.0[2] > 30 && p.0[1] > 20)
                .count();
            if colored > 100 {
                if let Ok(path) = std::env::var("DIMENSIFY_POINTCLOUD_CAPTURE") {
                    let mut ppm = b"P6\n256 256\n255\n".to_vec();
                    for pixel in image.pixels() {
                        ppm.extend_from_slice(&pixel.0[..3]);
                    }
                    std::fs::write(path, ppm).unwrap();
                }
                assert_eq!(
                    app.world().get::<PointcloudStatus>(cloud).unwrap().phase,
                    PointcloudPhase::Ready
                );
                println!("Offscreen fixture: {colored} colored pixels in 256x256 frame");
                break;
            }
        }
        if !capture_pending
            && app
                .world()
                .get::<PointcloudStatus>(cloud)
                .is_some_and(|s| s.phase == PointcloudPhase::Ready)
        {
            capture_pending = true;
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(
                    |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                        captured.0 = Some(event.image.clone());
                    },
                );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(feature = "viewer")]
#[test]
#[ignore = "requires a native GPU; run explicitly for depth-pick changes"]
fn depth_pick_reads_back_the_rendered_point_surface() {
    use dimensify_camera::{
        InspectEnvironment, InspectPick,
        viewer::{DepthPickPlugin, NavigationPick, ViewerCamera},
    };
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<PipelinedRenderingPlugin>(),
    )
    .add_plugins((PointcloudPlugin::default(), DepthPickPlugin));
    let deadline = Instant::now() + Duration::from_secs(60);
    while app.plugins_state() != PluginsState::Ready {
        assert!(Instant::now() < deadline, "GPU initialization timed out");
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let target = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            256,
            256,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            ViewerCamera,
            RenderTarget::Image(target.into()),
            Transform::from_xyz(0., 0., 6.).looking_at(Vec3::ZERO, Vec3::Y),
            Msaa::Off,
            Tonemapping::None,
            NoIndirectDrawing,
            renderer::PointCloudVisibilitySettings {
                point_budget: Some(10_000),
                min_radius: Some(0.),
                ..default()
            },
        ))
        .id();
    app.world_mut()
        .resource_mut::<NavigationPick>()
        .set_camera(camera);
    let mut source = LoadPointcloud::mesh(Sphere::new(1.5).mesh().uv(64, 32));
    source.settings.splats.point_size = 8.;
    let cloud = app.world_mut().spawn(source).id();
    let mut ready_frames = 0;
    let hit = loop {
        assert!(Instant::now() < deadline, "Depth pick never resolved");
        app.update();
        let ready = app
            .world()
            .get::<PointcloudStatus>(cloud)
            .is_some_and(|s| s.phase == PointcloudPhase::Ready);
        ready_frames += usize::from(ready);
        // Give the first chunks a few frames to be drawn before sampling depth.
        if ready_frames > 5 {
            let world = app.world_mut();
            let (view, transform) = world
                .query::<(&Camera, &GlobalTransform)>()
                .get(world, camera)
                .map(|(c, t)| (c.clone(), *t))
                .unwrap();
            match world.resource_mut::<NavigationPick>().pick_hit(
                &view,
                &transform,
                Vec2::splat(128.),
            ) {
                InspectPick::Pending => {}
                InspectPick::Miss => panic!("Depth pick missed the rendered sphere"),
                InspectPick::Hit(hit) => break hit,
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    println!("Depth pick hit {:?}", hit.point);
    // The front of the 1.5-radius sphere faces the camera at z = 6. The pick
    // radius (12 px) spans about 0.17 world units at that depth.
    assert!(
        hit.point.x.abs() < 0.2 && hit.point.y.abs() < 0.2,
        "{hit:?}"
    );
    assert!((1.2..=1.6).contains(&hit.point.z), "{hit:?}");
    assert!(hit.normal.z > 0.99, "{hit:?}");
}
