//! Measures actual rendered point footprints, rather than duplicating shader math.
use super::*;
use renderer::PointSizeMode;

fn point_width(app: &mut App, depth: f32) -> usize {
    app.world_mut()
        .resource_mut::<NavigationConfig>()
        .override_pose = Some(CameraPose::from_rotation(
        Vec3::new(0., 0., depth),
        Quat::IDENTITY,
        depth,
        std::f32::consts::FRAC_PI_2,
    ));
    for _ in 0..20 {
        app.update();
    }
    app.world_mut().resource_mut::<Capture>().0 = None;
    let target = app.world().resource::<Target>().0.clone();
    app.world_mut().spawn(Screenshot::image(target)).observe(
        |event: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
            capture.0 = Some(event.image.clone());
        },
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.update();
        if let Some(image) = app.world_mut().resource_mut::<Capture>().0.take() {
            let image = image.try_into_dynamic().unwrap().into_rgba8();
            // Two isolated points sit on the middle scanline; UI is outside it.
            let mut longest = 0;
            let mut run = 0;
            for x in 0..640 {
                let p = image.get_pixel(x, 240).0;
                if p[2] > 70 && p[1] > 40 {
                    run += 1;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
            assert!(longest > 0, "no point on the centre scanline");
            return longest;
        }
        assert!(Instant::now() < deadline, "point-size capture timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires native GPU; checks actual point-size projection and missing-spacing fallback"]
fn point_size_projection_matches_units_and_source_spacing() {
    let mut mesh = Mesh::new(
        bevy::mesh::PrimitiveTopology::PointList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-1_f32, 0., 0.], [1., 0., 0.]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.3_f32, 0.6, 0.9, 1.]; 2]);
    let plugins = DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (640, 480).into(),
                ..default()
            }),
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        })
        .disable::<bevy::winit::WinitPlugin>()
        .disable::<PipelinedRenderingPlugin>();
    let mut app = viewer_app(LoadPointcloud::mesh(mesh), plugins);
    app.init_resource::<Capture>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1. / 60.,
        )))
        .add_systems(Startup, retarget.after(setup));
    while app.plugins_state() != PluginsState::Ready {
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
    app.finish();
    app.cleanup();
    let image = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            640,
            480,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    app.insert_resource(Target(image));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.update();
        if app
            .world_mut()
            .query::<&PointcloudStatus>()
            .iter(app.world())
            .any(|s| s.phase == PointcloudPhase::Ready)
        {
            break;
        }
        assert!(Instant::now() < deadline, "cloud did not load");
    }
    app.insert_resource(settings::Settings::new(renderer::SplatSettings {
        point_size_mode: PointSizeMode::ScreenPixels,
        point_size: 16.,
        min_point_size: None,
        max_point_size: None,
        adaptive_point_size: false,
        radius: Some(0.5),
        ..default()
    }));
    let near = point_width(&mut app, 8.);
    let far = point_width(&mut app, 16.);
    println!("Screen pixels: near={near}, far={far}");
    assert!(
        (near as i32 - 16).abs() <= 1 && (far as i32 - 16).abs() <= 1,
        "16 screen pixels must remain 16 pixels across zoom; got {near}, {far}"
    );
    app.insert_resource(settings::Settings::new(renderer::SplatSettings {
        point_size_mode: PointSizeMode::WorldSpace,
        point_size: 0.4,
        min_point_size: None,
        max_point_size: None,
        adaptive_point_size: true,
        radius: Some(0.5),
        ..default()
    }));
    // The mesh loader has no spacing: adaptive must fall back to the chosen
    // manual size, not an invented world-space spacing of one.
    let near = point_width(&mut app, 8.);
    let far = point_width(&mut app, 16.);
    println!("World units without source spacing: near={near}, far={far}");
    assert!(
        (near as i32 - 12).abs() <= 1 && (far as i32 - 6).abs() <= 1,
        "0.4 world units should project to 12 and 6 pixels; got {near}, {far}"
    );
    let handle = app
        .world_mut()
        .query::<&renderer::PointCloud3d>()
        .single(app.world())
        .unwrap()
        .0
        .clone();
    app.world_mut()
        .resource_mut::<Assets<renderer::PointCloud>>()
        .get_mut(&handle)
        .unwrap()
        .spacing = Some(0.2);
    let near = point_width(&mut app, 8.);
    let far = point_width(&mut app, 16.);
    println!("Adaptive with source spacing: near={near}, far={far}");
    assert!(
        (near as i32 - 10).abs() <= 1 && (far as i32 - 5).abs() <= 1,
        "adaptive spacing 0.2 should project to about 10 and 5 pixels; got {near}, {far}"
    );
    *app.world_mut()
        .query_filtered::<&mut Projection, With<ViewerCamera>>()
        .single_mut(app.world_mut())
        .unwrap() = Projection::Orthographic(OrthographicProjection {
        scaling_mode: bevy::camera::ScalingMode::FixedVertical {
            viewport_height: 8.,
        },
        ..OrthographicProjection::default_3d()
    });
    app.insert_resource(settings::Settings::new(renderer::SplatSettings {
        point_size_mode: PointSizeMode::ScreenPixels,
        point_size: 16.,
        min_point_size: None,
        max_point_size: None,
        adaptive_point_size: false,
        radius: Some(0.5),
        ..default()
    }));
    let near = point_width(&mut app, 8.);
    let far = point_width(&mut app, 16.);
    println!("Orthographic screen pixels: near={near}, far={far}");
    assert!(
        (near as i32 - 16).abs() <= 1 && (far as i32 - 16).abs() <= 1,
        "orthographic screen pixels must be depth independent; got {near}, {far}"
    );
}
