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
    render_fixture(FixtureMaterial::Unlit, false);
}

#[test]
#[ignore = "requires a native GPU; run explicitly for point-cloud shader changes"]
fn materials_and_prepasses_render_offscreen() {
    for material in [
        FixtureMaterial::Unlit,
        FixtureMaterial::Lit,
        FixtureMaterial::Simple,
    ] {
        render_fixture(material, true);
    }
}

#[derive(Clone, Copy, Debug)]
enum FixtureMaterial {
    Unlit,
    Lit,
    Simple,
}

fn render_fixture(material: FixtureMaterial, prepasses: bool) {
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
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderTarget::Image(target.clone().into()),
            Transform::from_xyz(0., 0., 6.).looking_at(Vec3::ZERO, Vec3::Y),
            Msaa::Off,
            Tonemapping::Linear,
            NoIndirectDrawing,
            renderer::PointCloudVisibilitySettings {
                point_budget: Some(10_000),
                min_radius: Some(0.),
                ..default()
            },
        ))
        .id();
    if prepasses {
        app.world_mut().entity_mut(camera).insert((
            bevy::core_pipeline::prepass::DepthPrepass,
            bevy::core_pipeline::prepass::NormalPrepass,
            bevy::core_pipeline::prepass::MotionVectorPrepass,
        ));
    }
    if matches!(material, FixtureMaterial::Lit) {
        app.world_mut().spawn((
            DirectionalLight {
                illuminance: 10_000.,
                ..default()
            },
            Transform::from_xyz(3., 4., 5.).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
    let mut mesh = Sphere::new(1.5).mesh().uv(32, 16);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[0.2_f32, 0.7, 0.9, 1.]; mesh.count_vertices()],
    );
    let mut source = LoadPointcloud::mesh(mesh);
    source.settings.unlit = !matches!(material, FixtureMaterial::Lit);
    let cloud = app.world_mut().spawn(source).id();
    let mut simple_installed = false;
    let mut capture_pending = false;
    loop {
        assert!(
            Instant::now() < deadline,
            "No colored points rendered: {:?}",
            app.world().get::<PointcloudStatus>(cloud)
        );
        app.update();
        if matches!(material, FixtureMaterial::Simple) && !simple_installed {
            if app
                .world()
                .get::<renderer::PointCloudMaterial3d<renderer::StandardPointCloudMaterial>>(cloud)
                .is_some()
            {
                let handle = app
                    .world_mut()
                    .resource_mut::<Assets<renderer::SimplePointCloudMaterial>>()
                    .add(renderer::SimplePointCloudMaterial::default());
                app.world_mut()
                    .entity_mut(cloud)
                    .remove::<renderer::PointCloudMaterial3d<renderer::StandardPointCloudMaterial>>(
                    )
                    .insert(renderer::PointCloudMaterial3d(handle));
                simple_installed = true;
            }
            continue;
        }
        let pipelines_ready = pipelines_ready(&app);
        if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
            capture_pending = false;
            if !pipelines_ready {
                continue;
            }
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
                println!(
                    "Offscreen fixture ({material:?}, prepasses={prepasses}): {colored} colored pixels in 256x256 frame"
                );
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

fn pipelines_ready(app: &App) -> bool {
    use bevy::render::{
        RenderApp,
        render_resource::{CachedPipelineState, PipelineCache},
    };
    let cache = app.sub_app(RenderApp).world().resource::<PipelineCache>();
    let mut ready = true;
    for pipeline in cache.pipelines() {
        match &pipeline.state {
            CachedPipelineState::Err(
                bevy::shader::ShaderCacheError::ShaderNotLoaded(_)
                | bevy::shader::ShaderCacheError::ShaderImportNotYetAvailable,
            ) => ready = false,
            CachedPipelineState::Err(error) => panic!("Shader pipeline failed: {error}"),
            CachedPipelineState::Ok(_) => {}
            _ => ready = false,
        }
    }
    ready
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
            Tonemapping::Linear,
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

/// Orbits a real Potree dataset (`DIMENSIFY_POTREE_PATH`) under a small cache and checks that
/// eviction keeps residency bounded, frees chunk assets and lets refinement continue.
#[test]
#[ignore = "requires a native GPU and DIMENSIFY_POTREE_PATH; run explicitly for eviction changes"]
fn potree_eviction_bounds_residency_while_orbiting() {
    use bevy::{asset::AssetEvent, ecs::message::Messages};
    let Ok(path) = std::env::var("DIMENSIFY_POTREE_PATH") else {
        eprintln!("DIMENSIFY_POTREE_PATH is unset; skipping");
        return;
    };
    const CAP: usize = 64 * 1024 * 1024;
    // Eviction also holds residency to twice the camera's point budget.
    const BUDGET: usize = 1_000_000;
    const POINT_CAP: usize = 2 * BUDGET;
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
    .add_plugins(PointcloudPlugin {
        cache: renderer::PointCloudServerSettings {
            max_size: CAP,
            ..default()
        },
    });
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
            512,
            512,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            Msaa::Off,
            NoIndirectDrawing,
            renderer::PointCloudVisibilitySettings {
                point_budget: Some(BUDGET),
                min_radius: Some(55.),
                ..default()
            },
        ))
        .id();
    let cloud = app
        .world_mut()
        .spawn(LoadPointcloud::new(PointcloudSource::PotreePath(
            path.into(),
        )))
        .id();

    let bounds = loop {
        assert!(Instant::now() < deadline, "Potree metadata never loaded");
        app.update();
        let world = app.world();
        if let (Some(bounds), Some(transform)) = (
            world.get::<PointcloudStatus>(cloud).and_then(|s| s.bounds),
            world.get::<GlobalTransform>(cloud),
        ) {
            let center = transform.transform_point(bounds.center.into());
            let radius = (transform.affine().matrix3 * bounds.half_extents).length();
            break (center, radius);
        }
    };

    let resident_nodes = |world: &World| -> usize {
        world
            .resource::<Assets<renderer::PointCloud>>()
            .iter()
            .filter_map(|(_, cloud)| cloud.topology.as_octree())
            .map(|octree| octree.iter_loaded_chunks().count())
            .sum()
    };
    let (mut peak, mut capped_at, mut loads_after_cap) = (0, None, 0);
    let mut chunk_events = 0;
    let orbit = Duration::from_secs(40);
    let start = Instant::now();
    while start.elapsed() < orbit {
        let t = start.elapsed().as_secs_f32();
        // Orbit while swinging between close-up and overview, so detail must be swapped.
        let distance = bounds.1 * (0.6 + 0.9 * (0.5 + 0.5 * (t * 0.7).sin()));
        let eye = bounds.0 + Vec3::new(t.cos(), 0.35, t.sin()) * distance;
        *app.world_mut().get_mut::<Transform>(camera).unwrap() =
            Transform::from_translation(eye).looking_at(bounds.0, Vec3::Y);
        app.update();
        let world = app.world();
        let resident = world.resource::<renderer::PointCloudTotalSize>();
        peak = peak.max(resident.resident_bytes());
        let events = world.resource::<Messages<AssetEvent<renderer::PointCloudChunk>>>();
        let added = events
            .iter_current_update_messages()
            .filter(|event| matches!(event, AssetEvent::Added { .. }))
            .count();
        chunk_events += added;
        let full = resident.resident_bytes() * 10 >= CAP * 9
            || resident.resident_points() * 10 >= POINT_CAP * 9;
        if capped_at.is_none() && full {
            capped_at = Some(start.elapsed());
        } else if capped_at.is_some() {
            loads_after_cap += added;
        }
    }
    // Let dropped handles reach their asset stores.
    for _ in 0..10 {
        app.update();
    }
    let world = app.world();
    let resident = world.resource::<renderer::PointCloudTotalSize>();
    let nodes = resident_nodes(world);
    let chunks = world.resource::<Assets<renderer::PointCloudChunk>>().len();
    println!(
        "Potree eviction: {chunk_events} chunks loaded, peak {} MB, cap reached at {capped_at:?}, \
         {loads_after_cap} loads after cap, final {} MB / {:.2}M pts in {nodes} nodes, \
         {chunks} chunk assets",
        peak >> 20,
        resident.resident_bytes() >> 20,
        resident.resident_points() as f64 / 1e6,
    );
    // One frame of in-flight loads may land before the next eviction pass.
    assert!(peak <= CAP + 16 * 1024 * 1024, "residency exceeded the cap");
    assert!(
        resident.resident_points() <= POINT_CAP,
        "residency exceeded twice the point budget"
    );
    assert!(
        capped_at.is_none() || loads_after_cap > 0,
        "refinement stalled once the cache filled"
    );
    assert!(
        chunks <= nodes + 16,
        "{chunks} chunk assets outlive {nodes} resident nodes"
    );
}

/// Renders fixed views of `DIMENSIFY_POTREE_PATH` and prints selection metrics, for comparing
/// refinement settings. `DIMENSIFY_LOD_RADIUS` sets the minimum node radius (default 55 px),
/// `DIMENSIFY_LOD_OUT` receives one PNG per view.
#[test]
#[ignore = "requires a native GPU and DIMENSIFY_POTREE_PATH; run explicitly to compare LOD"]
fn potree_lod_comparison_captures() {
    let Ok(path) = std::env::var("DIMENSIFY_POTREE_PATH") else {
        eprintln!("DIMENSIFY_POTREE_PATH is unset; skipping");
        return;
    };
    let min_radius = std::env::var("DIMENSIFY_LOD_RADIUS").map_or(55., |r| r.parse().unwrap());
    let out = std::env::var("DIMENSIFY_LOD_OUT")
        .ok()
        .map(std::path::PathBuf::from);
    let (width, height) = (1000, 760);
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
            width,
            height,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
    let settings = renderer::PointCloudVisibilitySettings {
        // `DIMENSIFY_LOD_BUDGET` overrides the 5M point budget.
        point_budget: Some(
            std::env::var("DIMENSIFY_LOD_BUDGET").map_or(5_000_000, |b| b.parse().unwrap()),
        ),
        min_radius: Some(min_radius),
        ..default()
    };
    let camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            RenderTarget::Image(target.clone().into()),
            Msaa::Off,
            Tonemapping::None,
            NoIndirectDrawing,
            settings,
        ))
        .id();
    let mut request = LoadPointcloud::new(PointcloudSource::PotreePath(path.into()));
    request.up = PointcloudUp::Z;
    // `DIMENSIFY_LOD_FIXED_PX` draws fixed-size points instead of adaptive ones.
    if let Ok(pixels) = std::env::var("DIMENSIFY_LOD_FIXED_PX") {
        request.settings.splats.adaptive_point_size = false;
        request.settings.splats.point_size = pixels.parse().unwrap();
    }
    let cloud = app.world_mut().spawn(request).id();
    let (center, radius) = loop {
        assert!(Instant::now() < deadline, "Potree metadata never loaded");
        app.update();
        let world = app.world();
        if let (Some(bounds), Some(transform)) = (
            world.get::<PointcloudStatus>(cloud).and_then(|s| s.bounds),
            world.get::<GlobalTransform>(cloud),
        ) {
            let center = transform.transform_point(bounds.center.into());
            let radius = (transform.affine().matrix3 * bounds.half_extents).length();
            break (center, radius);
        }
    };
    let direction = Vec3::new(1., 0.6, 1.).normalize();
    let views = [
        ("overview", 1.0),
        ("mid", 0.45),
        ("close", 0.15),
        ("overview-again", 1.0),
    ];
    for (name, factor) in views {
        let eye = center + direction * radius * factor;
        *app.world_mut().get_mut::<Transform>(camera).unwrap() =
            Transform::from_translation(eye).looking_at(center, Vec3::Y);
        let start = Instant::now();
        let mut idle = 0;
        while idle < 60 && start.elapsed() < Duration::from_secs(45) {
            app.update();
            let status = app.world().get::<PointcloudStatus>(cloud).unwrap();
            idle = if status.active == 0 && status.queued == 0 {
                idle + 1
            } else {
                0
            };
        }
        let settled = start.elapsed();
        app.world_mut()
            .spawn(Screenshot::image(target.clone()))
            .observe(
                |event: On<ScreenshotCaptured>, mut captured: ResMut<Captured>| {
                    captured.0 = Some(event.image.clone());
                },
            );
        let image = loop {
            app.update();
            if let Some(image) = app.world_mut().resource_mut::<Captured>().0.take() {
                break image.try_into_dynamic().unwrap().into_rgba8();
            }
        };
        let covered = image.pixels().filter(|p| p.0[..3] != [0, 0, 0]).count();
        let world = app.world();
        let view = world
            .get::<renderer::VisiblePointCloudOctreeEntities>(camera)
            .unwrap();
        let clouds = world.resource::<Assets<renderer::PointCloud>>();
        let (mut nodes, mut points, mut depth_points, mut capped) = (0, 0, 0., 0);
        for entry in view.entities.values() {
            let Some(tree) = clouds
                .get(entry.asset_id)
                .and_then(|cloud| cloud.topology.as_octree())
            else {
                continue;
            };
            for node in entry
                .node_entities
                .iter()
                .filter_map(|n| tree.get_node(n.id))
            {
                nodes += 1;
                points += node.point_count;
                depth_points += node.depth as f64 * node.point_count as f64;
            }
            capped += entry.refinement_candidates.iter().filter(|c| c.1).count();
        }
        // Nearest drawn nodes: are they leaves, or did refinement stop above them?
        if std::env::var_os("DIMENSIFY_LOD_NEAREST").is_some() {
            let cloud_transform = world.get::<GlobalTransform>(cloud).unwrap();
            for entry in view.entities.values() {
                let Some(tree) = clouds
                    .get(entry.asset_id)
                    .and_then(|cloud| cloud.topology.as_octree())
                else {
                    continue;
                };
                let drawn: std::collections::HashSet<_> =
                    entry.node_entities.iter().map(|n| n.id).collect();
                let capped: std::collections::HashSet<_> = entry
                    .refinement_candidates
                    .iter()
                    .filter(|c| c.1)
                    .map(|c| c.0)
                    .collect();
                let mut nearest: Vec<_> = drawn
                    .iter()
                    .filter_map(|&id| tree.get_node(id))
                    .filter_map(|node| {
                        let aabb = node.aabb?;
                        let local = cloud_transform.affine().inverse().transform_point3(eye);
                        let gap =
                            (local - Vec3::from(aabb.center)).abs() - Vec3::from(aabb.half_extents);
                        Some((gap.max(Vec3::ZERO).length(), node))
                    })
                    .collect();
                nearest.sort_by(|a, b| a.0.total_cmp(&b.0));
                for (distance, node) in nearest.iter().take(12) {
                    let children: Vec<_> = node
                        .children_mask
                        .iter_one_bits()
                        .map(|i| node.children[i as usize])
                        .collect();
                    let drawn_children = children.iter().filter(|c| drawn.contains(c)).count();
                    let capped_children = children.iter().filter(|c| capped.contains(c)).count();
                    let unloaded = children
                        .iter()
                        .filter_map(|&c| tree.get_node(c))
                        .filter(|c| c.chunk.is_none())
                        .count();
                    println!(
                        "  {:<12} depth {} {:>6} pts, {:>5.2} m away, children {} (drawn {}, capped {}, unloaded {})",
                        node.name,
                        node.depth,
                        node.point_count,
                        distance,
                        children.len(),
                        drawn_children,
                        capped_children,
                        unloaded,
                    );
                }
            }
        }
        // Why drawn nodes' children were not drawn, by depth of the drawn parent.
        if std::env::var_os("DIMENSIFY_LOD_WHY").is_some() {
            let frustum = world
                .get::<bevy::camera::primitives::Frustum>(camera)
                .unwrap();
            let cloud_transform = world.get::<GlobalTransform>(cloud).unwrap();
            let world_from_local = cloud_transform.affine();
            for entry in view.entities.values() {
                let Some(tree) = clouds
                    .get(entry.asset_id)
                    .and_then(|cloud| cloud.topology.as_octree())
                else {
                    continue;
                };
                let drawn: std::collections::HashSet<_> =
                    entry.node_entities.iter().map(|n| n.id).collect();
                let candidates: std::collections::HashMap<_, _> =
                    entry.refinement_candidates.iter().copied().collect();
                // depth -> [leaf parents, drawn, capped, pending, culled, other]
                let mut table = std::collections::BTreeMap::<u32, [usize; 6]>::new();
                for node in drawn.iter().filter_map(|&id| tree.get_node(id)) {
                    let row = table.entry(node.depth).or_default();
                    if node.children_mask.is_empty() {
                        row[0] += 1;
                    }
                    for child in node
                        .children_mask
                        .iter_one_bits()
                        .filter_map(|i| tree.get_node(node.children[i as usize]))
                    {
                        let visible = child.aabb.is_some_and(|aabb| {
                            frustum.intersects_obb(&aabb, &world_from_local, true, false)
                        });
                        let column = if drawn.contains(&child.id) {
                            1
                        } else if candidates.get(&child.id) == Some(&true) {
                            2
                        } else if candidates.contains_key(&child.id) {
                            3
                        } else if !visible {
                            4
                        } else {
                            5
                        };
                        row[column] += 1;
                    }
                }
                println!(
                    "  depth: leaf-parents | children drawn, capped, pending, culled, filtered"
                );
                for (depth, row) in table {
                    println!(
                        "  {depth}: {:>4} | {:>5} {:>5} {:>5} {:>5} {:>5}",
                        row[0], row[1], row[2], row[3], row[4], row[5]
                    );
                }
            }
        }
        let status = world.get::<PointcloudStatus>(cloud).unwrap();
        println!(
            "LOD {name:>14}: {nodes:>4} nodes, {:>5.2}M pts, mean depth {:.2}, {capped:>4} capped, \
             coverage {:>5.1}%, settled {:>5.1}s (active {}, queued {})",
            points as f64 / 1e6,
            depth_points / points.max(1) as f64,
            100. * covered as f64 / (width * height) as f64,
            settled.as_secs_f32(),
            status.active,
            status.queued,
        );
        if let Some(out) = &out {
            std::fs::create_dir_all(out).unwrap();
            let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
            for pixel in image.pixels() {
                ppm.extend_from_slice(&pixel.0[..3]);
            }
            std::fs::write(out.join(format!("{name}.ppm")), ppm).unwrap();
        }
    }
}
