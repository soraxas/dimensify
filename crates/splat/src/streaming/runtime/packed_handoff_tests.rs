//! Native offscreen compact SOG contract, sharing the ordinary raster harness.
use super::*;

#[test]
#[ignore = "native offscreen GPU residency without CPU staging; no window; run explicitly"]
fn packed_gpu_residency_survives_cpu_cache_eviction() {
    use crate::renderer::{
        SnapshotRenderer, SnapshotState, Submission,
        gpu::{GpuBudget, SogRendererBackend},
        packed::PackedResident,
        source::SourceStorage,
    };
    use bevy::ecs::system::SystemState;
    let (mut app, target, camera) = render_app(32);
    app.insert_resource(SogRendererBackend::Packed);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let position = Vec3::new(0., 0., -2.);
    let mut data = stream(position, now);
    data.entries[0].loading = Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
    data.entries[0].retry_at = f64::INFINITY;
    let scene = app
        .world_mut()
        .spawn((data, Transform::IDENTITY, Visibility::Inherited))
        .id();
    until(&mut app, "resident coarse source", |app| {
        app.world()
            .get::<SogStream>(scene)
            .is_some_and(|s| s.displayed.iter().any(|r| r.file == 0))
    });
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    app.world_mut().get_mut::<SogStream>(scene).unwrap().entries[1].loading =
        Some(chunk(position, true, now).with_storage(SourceStorage::Packed));
    until(&mut app, "resident fine source", |app| {
        app.world()
            .get::<SogStream>(scene)
            .is_some_and(|s| s.displayed.iter().any(|r| r.file == 1))
    });
    let uploads = app
        .world()
        .resource::<PackedResident>()
        .statistics
        .uploaded_bytes;
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(scene).unwrap();
        // The inactive coarse file remains on the GPU, but its CPU cache record
        // has been evicted. A new selection must recover it without transport.
        stream.entries[0].ready = None;
        stream.next_update = 0.;
    }
    app.world_mut()
        .resource_mut::<SogLodSettings>()
        .splat_budget = 1;
    app.update();
    assert!(
        app.world().get::<SogStream>(scene).unwrap().entries[0]
            .ready
            .is_some(),
        "CPU cache eviction hid a GPU-ready source from LOD selection"
    );
    for (budget, file, channel) in [(1, 0, 0), (32, 1, 1)] {
        app.world_mut()
            .resource_mut::<SogLodSettings>()
            .splat_budget = budget;
        until(&mut app, "GPU-only source reselection", |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| s.displayed.iter().any(|r| r.file == file))
        });
        assert!(coloured_pixels(&capture(&mut app, &target), channel) > 10);
    }
    let stream = app.world().get::<SogStream>(scene).unwrap();
    assert!(stream.entries.iter().all(|e| e.loading.is_none()));
    assert!(
        stream
            .entries
            .iter()
            .all(|e| e.ready.as_ref().unwrap().cloud.packed().is_err()),
        "fully uploaded sources must release CPU texture pixels"
    );
    assert_eq!(
        app.world()
            .resource::<PackedResident>()
            .statistics
            .uploaded_bytes,
        uploads
    );

    // A second scene puts real pressure on the same global source cache.
    // Coarse fallback and displayed fine detail must both remain protected.
    let cap = app
        .world()
        .resource::<PackedResident>()
        .statistics
        .resident_bytes;
    app.world_mut().resource_mut::<GpuBudget>().source_bytes = cap;
    let other = app.world_mut().spawn_empty().id();
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let replacement = chunk(position, false, now)
        .with_storage(SourceStorage::Packed)
        .poll_measured(now, &SogCpuTimings::default())
        .unwrap()
        .unwrap();
    let mut adapter = SystemState::<SnapshotRenderer>::new(app.world_mut());
    let mut snapshot = SnapshotState::default();
    let ranges = [SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    }];
    assert!(
        adapter
            .get_mut(app.world_mut())
            .unwrap()
            .submit(&mut snapshot, other, camera, &ranges, |_| Some(
                &replacement
            ),)
            .is_err(),
        "GPU admission must not evict mandatory coarse fallback to fit another scene"
    );
    adapter.apply(app.world_mut());
    assert_eq!(
        adapter
            .get_mut(app.world_mut())
            .unwrap()
            .source_cache_bytes(other),
        Some(0),
        "other scenes must account for both displayed and baseline GPU pins"
    );

    // Once coarse is displayed, inactive fine data (not fallback) can yield.
    app.world_mut()
        .resource_mut::<SogLodSettings>()
        .splat_budget = 1;
    until(&mut app, "coarse fallback activated", |app| {
        app.world()
            .get::<SogStream>(scene)
            .is_some_and(|s| s.displayed.iter().any(|r| r.file == 0))
    });
    capture(&mut app, &target); // Finish fine render-world retirement.
    let mut fine = app.world_mut().get_mut::<SogStream>(scene).unwrap().entries[1]
        .ready
        .take();
    let mut submitted = false;
    for _ in 0..32 {
        app.world_mut()
            .resource_mut::<bevy::diagnostic::FrameCount>()
            .0 += 1;
        let result = adapter
            .get_mut(app.world_mut())
            .unwrap()
            .submit(
                &mut snapshot,
                other,
                camera,
                &[SplatRange {
                    file: 0,
                    offset: 0,
                    count: 1,
                }],
                |_| Some(&replacement),
            )
            .unwrap();
        adapter.apply(app.world_mut());
        if result == Submission::Submitted {
            submitted = true;
            break;
        }
    }
    assert!(
        submitted,
        "unpinned GPU source must yield to a new scene under pressure"
    );
    assert!(
        adapter
            .get_mut(app.world_mut())
            .unwrap()
            .sync_source(scene, 1, &mut fine)
    );
    assert!(
        fine.is_none(),
        "evicted source must not remain selectable without a reload"
    );
    assert!(
        app.world()
            .resource::<PackedResident>()
            .statistics
            .resident_bytes
            <= cap
    );
    adapter
        .get_mut(app.world_mut())
        .unwrap()
        .clear(&mut snapshot);
    adapter.apply(app.world_mut());
    let reupload_start = app
        .world()
        .resource::<PackedResident>()
        .statistics
        .uploaded_bytes;
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(scene).unwrap();
        // Supply the captured transport response for a genuinely evicted file;
        // exercise the ordinary decoder, selection, upload and readiness path.
        stream.entries[1].loading =
            Some(chunk(position, true, now).with_storage(SourceStorage::Packed));
        stream.next_update = 0.;
    }
    app.world_mut()
        .resource_mut::<SogLodSettings>()
        .splat_budget = 32;
    until(
        &mut app,
        "evicted file decodes and becomes visible again",
        |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| s.displayed.iter().any(|r| r.file == 1))
        },
    );
    assert!(coloured_pixels(&capture(&mut app, &target), 1) > 10);
    let stats = app.world().resource::<PackedResident>().statistics;
    assert!(
        stats.uploaded_bytes > reupload_start,
        "genuine GPU eviction must upload again"
    );
    assert!(stats.resident_bytes <= cap);
    app.world_mut().despawn(scene);
    app.world_mut().despawn(other);
    until(
        &mut app,
        "GPU descriptors do not pin retired sources",
        |app| {
            app.world()
                .resource::<PackedResident>()
                .statistics
                .resident_bytes
                == 0
        },
    );
}

#[test]
#[ignore = "native offscreen persistent scratch resize; no window; run explicitly"]
fn persistent_scratch_capacity_shrinks_and_explicit_retirement_releases_it() {
    use bevy::render::RenderApp;
    use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d, sort::radix::RadixSortBuffers};
    let (mut app, target, _) = render_app(32);
    let handle = app
        .world()
        .resource::<Assets<PlanarGaussian3d>>()
        .reserve_handle();
    let id = handle.id();
    let mut old = None;
    for count in [128, 32] {
        app.sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<RadixSortBuffers<Gaussian3d>>()
            .retain_persistent(id, count);
        capture(&mut app, &target);
        let scratch = &app
            .sub_app(RenderApp)
            .world()
            .resource::<RadixSortBuffers<Gaussian3d>>()
            .asset_map[&id];
        assert_eq!(scratch.entry_buffer_b.size(), count as u64 * 8);
        assert_ne!(
            Some(scratch.entry_buffer_b.id()),
            old,
            "capacity reduction must reclaim oversized scratch"
        );
        old = Some(scratch.entry_buffer_b.id());
    }
    app.sub_app_mut(RenderApp)
        .world_mut()
        .resource_mut::<RadixSortBuffers<Gaussian3d>>()
        .release_persistent(id);
    capture(&mut app, &target);
    assert!(
        app.sub_app(RenderApp)
            .world()
            .resource::<RadixSortBuffers<Gaussian3d>>()
            .asset_map
            .is_empty()
    );
}

#[test]
#[ignore = "native offscreen projector failure/recovery; no window; run explicitly"]
fn failed_projection_compilation_falls_back_and_can_recover() {
    use crate::{
        renderer::{gpu::SogRendererBackend, source::SourceStorage},
        runtime::SogProjectionCache,
    };
    use bevy::render::{
        RenderApp,
        render_resource::{CachedPipelineState, PipelineCache, PipelineDescriptor},
    };
    use bevy_gaussian_splatting::render::{compact::CompactCloud, projected};

    let (mut app, target, _) = render_app(32);
    app.insert_resource(SogRendererBackend::Packed);
    app.insert_resource(SogProjectionCache(true));
    let original_shader = app
        .world()
        .resource::<Assets<Shader>>()
        .get(&projected::SHADER)
        .unwrap()
        .clone();
    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(
            projected::SHADER.id(),
            Shader::from_wgsl("intentionally invalid WGSL", "projection-failure-test.wgsl"),
        )
        .unwrap();
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let position = Vec3::new(0., 0., -2.);
    let mut data = stream(position, now);
    data.entries[0].loading = Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
    let scene = app
        .world_mut()
        .spawn((data, Transform::IDENTITY, Visibility::Inherited))
        .id();
    until(&mut app, "injected projector compilation failure", |app| {
        app.sub_app(RenderApp).world().resource::<PipelineCache>().pipelines().any(|p| {
            matches!(&p.descriptor, PipelineDescriptor::ComputePipelineDescriptor(d) if d.shader == projected::SHADER)
                && matches!(&p.state, CachedPipelineState::Err(_))
        })
    });
    until(
        &mut app,
        "failed projector must not strand first generation",
        |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| !s.displayed.is_empty())
        },
    );
    let (entity, requested) = app
        .world_mut()
        .query::<(Entity, &CompactCloud)>()
        .single(app.world())
        .map(|(e, c)| (e, c.has_projection_cache()))
        .unwrap();
    assert!(
        requested,
        "fallback must preserve the producer's requested configuration"
    );
    let render_entity = app
        .world()
        .get::<bevy::render::sync_world::RenderEntity>(entity)
        .unwrap()
        .id();
    let effective = app
        .sub_app(RenderApp)
        .world()
        .get::<CompactCloud>(render_entity)
        .unwrap();
    assert!(!effective.has_projection_cache());
    assert!(effective.is_prepared());
    let fallback = capture(&mut app, &target);
    assert!(coloured_pixels(&fallback, 0) > 10);

    app.world_mut()
        .resource_mut::<Assets<Shader>>()
        .insert(projected::SHADER.id(), original_shader)
        .unwrap();
    until(
        &mut app,
        "projector recovery after corrected shader",
        |app| {
            app.sub_app(RenderApp)
                .world()
                .get::<CompactCloud>(render_entity)
                .is_some_and(|c| c.has_projection_cache() && c.is_prepared())
        },
    );
    let recovered = capture(&mut app, &target);
    assert!(coloured_pixels(&recovered, 0) > 10);
    assert!(
        fallback
            .iter()
            .zip(&recovered)
            .all(|(a, b)| a.abs_diff(*b) <= 1)
    );
}

#[test]
#[ignore = "native offscreen lazy projector compilation; no window; run explicitly"]
fn projection_pipeline_is_only_compiled_for_opted_in_supported_work() {
    use crate::renderer::{gpu::SogRendererBackend, source::SourceStorage};
    use bevy::render::{
        RenderApp,
        render_resource::{PipelineCache, PipelineDescriptor},
        renderer::RenderDevice,
    };
    use bevy_gaussian_splatting::{
        CloudSettings,
        render::{compact::CompactCloud, projected},
    };

    let (mut app, target, _) = render_app(32);
    let pipeline_count = |app: &App| {
        app.sub_app(RenderApp)
            .world()
            .resource::<PipelineCache>()
            .pipelines()
            .filter(|p| {
                matches!(&p.descriptor,
                PipelineDescriptor::ComputePipelineDescriptor(d) if d.shader == projected::SHADER)
            })
            .count()
    };
    capture(&mut app, &target);
    assert_eq!(
        pipeline_count(&app),
        0,
        "default startup compiled the opt-in projector"
    );

    app.insert_resource(SogRendererBackend::Packed);
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let position = Vec3::new(0., 0., -2.);
    let mut data = stream(position, now);
    data.entries[0].loading = Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
    let scene = app
        .world_mut()
        .spawn((data, Transform::IDENTITY, Visibility::Inherited))
        .id();
    until(&mut app, "ordinary packed work ready", |app| {
        app.world()
            .get::<SogStream>(scene)
            .is_some_and(|s| !s.displayed.is_empty())
    });
    let (entity, original) = app
        .world_mut()
        .query::<(Entity, &CompactCloud)>()
        .single(app.world())
        .map(|(e, c)| (e, c.clone()))
        .unwrap();
    assert!(!empty_image(&capture(&mut app, &target)));
    assert_eq!(
        pipeline_count(&app),
        0,
        "ordinary packed work compiled the projector"
    );

    let device = app.world().resource::<RenderDevice>().clone();
    let mut cached = original.clone();
    cached.enable_projection_cache(&device, 1024).unwrap();
    app.world_mut().entity_mut(entity).insert(cached.clone());
    app.world_mut()
        .get_mut::<CloudSettings>(entity)
        .unwrap()
        .aabb = true;
    assert!(!empty_image(&capture(&mut app, &target)));
    assert_eq!(
        pipeline_count(&app),
        0,
        "unsupported appearance compiled the projector"
    );

    app.world_mut()
        .get_mut::<CloudSettings>(entity)
        .unwrap()
        .aabb = false;
    until(&mut app, "lazy projector is compiled and draws", |app| {
        app.world()
            .get::<bevy::render::sync_world::RenderEntity>(entity)
            .and_then(|e| app.sub_app(RenderApp).world().get::<CompactCloud>(e.id()))
            .is_some_and(|c| c.has_projection_cache() && c.is_prepared())
    });
    assert!(!empty_image(&capture(&mut app, &target)));
    assert_eq!(pipeline_count(&app), 1);

    app.world_mut().entity_mut(entity).insert(original);
    capture(&mut app, &target);
    app.world_mut().entity_mut(entity).insert(cached);
    capture(&mut app, &target);
    assert_eq!(
        pipeline_count(&app),
        1,
        "re-enabling queued duplicate projector pipelines"
    );
}

#[test]
#[ignore = "native offscreen projection coverage oracle; no window; run explicitly"]
fn projected_footprint_preserves_edges_near_plane_and_projection_changes() {
    use crate::renderer::{gpu::SogRendererBackend, source::SourceStorage};
    use bevy::render::{RenderApp, renderer::RenderDevice};
    use bevy_gaussian_splatting::{CloudSettings, render::compact::CompactCloud};
    let (mut app, target, camera) = render_app(32);
    app.insert_resource(SogRendererBackend::Packed);
    let device = app.world().resource::<RenderDevice>().clone();
    let default_projection = app.world().get::<Projection>(camera).unwrap().clone();
    for position in [
        Vec3::new(0., 0., -2.),
        Vec3::new(-2.4, 0., -2.),
        Vec3::new(2.4, 0., -2.),
        Vec3::new(0., -2.4, -2.),
        Vec3::new(0., 2.4, -2.),
        Vec3::new(0., 0., -0.05),
    ] {
        let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
        let mut data = stream(position, now);
        data.entries[0].loading =
            Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
        let scene = app
            .world_mut()
            .spawn((data, Transform::IDENTITY, Visibility::Inherited))
            .id();
        until(&mut app, "projection oracle coarse ready", |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| !s.displayed.is_empty())
        });
        let (entity, original) = app
            .world_mut()
            .query::<(Entity, &CompactCloud)>()
            .single(app.world())
            .map(|(e, c)| (e, c.clone()))
            .unwrap();
        for projection in [
            default_projection.clone(),
            Projection::Perspective(PerspectiveProjection {
                fov: 150f32.to_radians(),
                ..default()
            }),
            Projection::Orthographic(OrthographicProjection {
                scale: 2.,
                ..OrthographicProjection::default_3d()
            }),
        ] {
            app.world_mut().entity_mut(camera).insert(projection);
            app.world_mut().entity_mut(entity).insert(original.clone());
            let reference = capture(&mut app, &target);
            let mut projected = original.clone();
            projected.enable_projection_cache(&device, 1024).unwrap();
            app.world_mut().entity_mut(entity).insert(projected);
            let deadline = Instant::now() + Duration::from_secs(30);
            let cached = loop {
                app.update();
                let world = app.sub_app_mut(RenderApp).world_mut();
                if world
                    .query::<&CompactCloud>()
                    .iter(world)
                    .any(|c| c.has_projection_cache() && c.is_prepared())
                {
                    // Request the screenshot after readiness, not before it.
                    // Readback completes later and can otherwise return a
                    // pre-compilation frame alongside current ready state.
                    break capture(&mut app, &target);
                }
                assert!(
                    Instant::now() < deadline,
                    "projection oracle cache not ready"
                );
            };
            let max = reference
                .iter()
                .zip(&cached)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            // Half-packed axes can move an edge pixel across the fragment's
            // 1/255 alpha cutoff. For this unit-red/black fixture that is exactly
            // ceil(sRGB(1/255)*255)=13, not evidence of lost bright coverage.
            let cutoff_srgb =
                ((1.055 * (1.0 / 255.0_f32).powf(1.0 / 2.4) - 0.055) * 255.0).ceil() as u8;
            let outliers: Vec<_> = reference
                .as_chunks::<4>()
                .0
                .iter()
                .zip(cached.as_chunks::<4>().0.iter())
                .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 8))
                .collect();
            assert!(
                outliers.len() <= 4
                    && outliers
                        .iter()
                        .all(|(a, b)| a[..3].iter().chain(&b[..3]).all(|v| *v <= cutoff_srgb)),
                "projection coverage at {position:?} changed by {max}"
            );
            // A profile unsupported by the cache must fall back to ordinary
            // vertex projection, not make an otherwise valid scene disappear.
            app.world_mut()
                .get_mut::<CloudSettings>(entity)
                .unwrap()
                .aabb = true;
            let deadline = Instant::now() + Duration::from_secs(30);
            let fallback = loop {
                let image = capture(&mut app, &target);
                let world = app.sub_app_mut(RenderApp).world_mut();
                if world
                    .query::<&CompactCloud>()
                    .iter(world)
                    .any(|c| !c.has_projection_cache() && c.is_prepared())
                {
                    break image;
                }
                assert!(
                    Instant::now() < deadline,
                    "unsupported projection settings did not fall back"
                );
            };
            app.world_mut().entity_mut(entity).insert(original.clone());
            let fallback_reference = capture(&mut app, &target);
            assert_eq!(
                fallback, fallback_reference,
                "unsupported projection settings changed raw coverage"
            );
            app.world_mut()
                .get_mut::<CloudSettings>(entity)
                .unwrap()
                .aabb = false;
        }
        app.world_mut().despawn(scene);
        capture(&mut app, &target);
    }
}

#[test]
#[ignore = "native offscreen packed pending teardown; no window; run explicitly"]
fn packed_pending_scene_removal_releases_unpublished_work() {
    use crate::renderer::{gpu::SogRendererBackend, packed::PackedResident, source::SourceStorage};
    let (mut app, target, _) = render_app(32);
    app.insert_resource(SogRendererBackend::Packed);
    for _ in 0..3 {
        let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
        let position = Vec3::new(0., 0., -2.);
        let mut data = stream(position, now);
        data.entries[0].loading =
            Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
        let scene = app
            .world_mut()
            .spawn((data, Transform::IDENTITY, Visibility::Inherited))
            .id();
        until(&mut app, "pending packed generation", |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| s.snapshot.is_pending())
        });
        assert!(
            app.world()
                .get::<SogStream>(scene)
                .unwrap()
                .displayed
                .is_empty()
        );
        app.world_mut().despawn(scene);
        until(&mut app, "pending source and slot retirement", |app| {
            let stats = app.world().resource::<PackedResident>().statistics;
            stats.resident_bytes == 0 && stats.selected_buffer_bytes == 0
        });
        assert!(empty_image(&capture(&mut app, &target)));
    }
}

#[test]
#[ignore = "native offscreen packed scene ownership; no window; run explicitly"]
fn packed_same_file_ids_are_isolated_across_scene_lifetimes() {
    use crate::renderer::{gpu::SogRendererBackend, packed::PackedResident, source::SourceStorage};
    let (mut app, target, _) = render_app(32);
    app.insert_resource(SogRendererBackend::Packed);
    let mut scenes = Vec::new();
    for x in [-0.35, 0.35] {
        let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
        let position = Vec3::new(x, 0., -2.);
        let mut data = stream(position, now);
        data.entries[0].loading =
            Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
        scenes.push(
            app.world_mut()
                .spawn((data, Transform::IDENTITY, Visibility::Inherited))
                .id(),
        );
    }
    until(&mut app, "two independently ready packed scenes", |app| {
        scenes.iter().all(|&scene| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| !s.displayed.is_empty())
        })
    });
    let all_pixels = coloured_pixels(&capture(&mut app, &target), 0);
    assert!(all_pixels > 20);
    let both = app.world().resource::<PackedResident>();
    let left_bytes = both.scene_source_bytes(scenes[0]);
    let right_bytes = both.scene_source_bytes(scenes[1]);
    assert!(left_bytes > 0);
    assert_eq!(left_bytes, right_bytes);
    assert_eq!(both.statistics.resident_bytes, left_bytes + right_bytes);
    app.world_mut().despawn(scenes[0]);
    until(&mut app, "first scene source retirement", |app| {
        app.world()
            .resource::<PackedResident>()
            .statistics
            .resident_bytes
            == right_bytes
    });
    let remaining = coloured_pixels(&capture(&mut app, &target), 0);
    assert!(
        remaining > 10 && remaining < all_pixels,
        "the other scene must stay visible"
    );
    assert_eq!(
        app.world()
            .resource::<PackedResident>()
            .scene_source_bytes(scenes[1]),
        right_bytes
    );
    app.world_mut().despawn(scenes[1]);
    until(&mut app, "all scene sources retired", |app| {
        app.world()
            .resource::<PackedResident>()
            .statistics
            .resident_bytes
            == 0
    });
    assert!(empty_image(&capture(&mut app, &target)));
}

#[test]
#[ignore = "native offscreen packed streaming; no window; run explicitly"]
fn packed_stream_refines_without_expanded_assets_and_retires_sources() {
    packed_stream_lifecycle(false);
}

#[test]
#[ignore = "native offscreen compute-projected streaming; no window; run explicitly"]
fn projected_stream_refines_reuses_slots_and_retires_cache() {
    packed_stream_lifecycle(true);
}

fn packed_stream_lifecycle(projected: bool) {
    use crate::renderer::{
        gpu::{GpuBudget, SogRendererBackend},
        packed::PackedResident,
        source::SourceStorage,
    };
    use bevy_gaussian_splatting::PlanarGaussian3d;
    let (mut app, target, _) = render_app(32);
    app.insert_resource(SogRendererBackend::Packed);
    app.insert_resource(crate::runtime::SogProjectionCache(projected));
    app.insert_resource(crate::diagnostics::sog_cache::SogCacheTrace::enabled());
    let scratch_ids = |app: &App| {
        use bevy_gaussian_splatting::{Gaussian3d, sort::radix::RadixSortBuffers};
        app.sub_app(bevy::render::RenderApp)
            .world()
            .resource::<RadixSortBuffers<Gaussian3d>>()
            .asset_map
            .iter()
            .map(|(id, buffers)| {
                let ids: Vec<_> = [
                    &buffers.sorting_global_buffer,
                    &buffers.sorting_status_counter_buffer,
                    &buffers.entry_buffer_b,
                    &buffers.dispatch_indirect_buffer,
                ]
                .into_iter()
                .chain(buffers.sorting_pass_buffers.iter())
                .map(|buffer| buffer.id())
                .collect();
                (*id, ids)
            })
            .collect::<HashMap<_, _>>()
    };
    app.world_mut()
        .resource_mut::<crate::runtime::SogRuntimeSettings>()
        .compaction = true;
    app.world_mut().resource_mut::<SogLodSettings>().cache_bytes = 16 * 1024 * 1024;
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let position = Vec3::new(0., 0., -2.);
    let mut data = stream(position, now);
    data.entries[0].loading = Some(chunk(position, false, now).with_storage(SourceStorage::Packed));
    let scene = app
        .world_mut()
        .spawn((data, Transform::IDENTITY, Visibility::Inherited))
        .id();
    until(&mut app, "packed coarse readiness", |app| {
        app.world()
            .get::<SogStream>(scene)
            .is_some_and(|s| s.displayed.iter().any(|r| r.file == 0))
    });
    assert!(coloured_pixels(&capture(&mut app, &target), 0) > 10);
    let coarse_scratch = scratch_ids(&app);
    assert_eq!(coarse_scratch.len(), 1);
    assert!(
        app.world()
            .resource::<Assets<PlanarGaussian3d>>()
            .is_empty()
    );
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(scene).unwrap();
        stream.entries[1].loading =
            Some(chunk(position, true, now).with_storage(SourceStorage::Packed));
    }
    // A lowered cap rejects refinement without losing the old visible image.
    let old_uploads = app
        .world()
        .resource::<PackedResident>()
        .statistics
        .uploaded_bytes;
    let old_budget = app.world().resource::<GpuBudget>().source_bytes;
    app.world_mut().resource_mut::<GpuBudget>().source_bytes = old_uploads;
    until(
        &mut app,
        "fine source decoded while GPU cap is full",
        |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| s.entries[1].ready.is_some())
        },
    );
    let held = capture(&mut app, &target);
    assert!(coloured_pixels(&held, 0) > 10);
    assert_eq!(coloured_pixels(&held, 1), 0);
    assert_eq!(
        app.world()
            .resource::<PackedResident>()
            .statistics
            .uploaded_bytes,
        old_uploads
    );
    app.world_mut().resource_mut::<GpuBudget>().source_bytes = old_budget;
    until(&mut app, "packed finer readiness", |app| {
        app.world()
            .get::<SogStream>(scene)
            .is_some_and(|s| s.displayed.iter().any(|r| r.file == 1))
    });
    let fine = capture(&mut app, &target);
    assert!(coloured_pixels(&fine, 1) > 10);
    assert_eq!(coloured_pixels(&fine, 0), 0, "old and new must never stack");
    let both_scratch = scratch_ids(&app);
    assert_eq!(
        both_scratch.len(),
        2,
        "inactive slot must retain its scratch"
    );
    for (id, buffers) in &coarse_scratch {
        assert_eq!(both_scratch.get(id), Some(buffers));
    }
    assert!(
        app.world()
            .resource::<Assets<PlanarGaussian3d>>()
            .is_empty()
    );
    assert!(
        app.world()
            .resource::<PackedResident>()
            .statistics
            .uploaded_bytes
            > 0
    );
    let uploads = app
        .world()
        .resource::<PackedResident>()
        .statistics
        .uploaded_bytes;
    for (budget, file, channel) in [(1, 0, 0), (32, 1, 1)] {
        app.world_mut()
            .resource_mut::<SogLodSettings>()
            .splat_budget = budget;
        until(&mut app, "packed slot reuse", |app| {
            app.world()
                .get::<SogStream>(scene)
                .is_some_and(|s| s.displayed.iter().any(|r| r.file == file))
        });
        assert!(coloured_pixels(&capture(&mut app, &target), channel) > 10);
        assert_eq!(
            scratch_ids(&app),
            both_scratch,
            "slot reuse recreated scratch"
        );
    }
    let stats = app.world().resource::<PackedResident>().statistics;
    assert_eq!(
        stats.uploaded_bytes, uploads,
        "resident sources must not upload again"
    );
    assert_eq!(stats.allocations, 2, "only two working slots");
    assert_eq!(
        stats.selected_buffer_bytes,
        2 * (32 * if projected { 64 } else { 32 } + 16)
    );
    assert!(stats.reuses >= 2);
    let lifecycle = app
        .world()
        .resource::<crate::diagnostics::sog_cache::SogCacheTrace>()
        .report()
        .unwrap();
    let uploads: Vec<_> = lifecycle
        .recent
        .iter()
        .filter(|e| e.kind == crate::diagnostics::sog_cache::Kind::Uploaded)
        .collect();
    assert_eq!(uploads.len(), 2, "cached sources must upload only once");
    assert!(
        uploads
            .iter()
            .all(|e| e.scene == scene.to_bits().to_string())
    );
    assert_eq!(uploads.iter().map(|e| e.source).collect::<Vec<_>>(), [0, 1]);
    app.world_mut().despawn(scene);
    until(&mut app, "packed source retirement", |app| {
        app.world()
            .resource::<PackedResident>()
            .statistics
            .resident_bytes
            == 0
    });
    assert!(empty_image(&capture(&mut app, &target)));
    assert!(
        scratch_ids(&app).is_empty(),
        "scene teardown must release scratch"
    );
    assert_eq!(
        app.world()
            .resource::<PackedResident>()
            .statistics
            .selected_buffer_bytes,
        0
    );
}

#[test]
#[ignore = "native offscreen compact SOG integration; no window; run explicitly"]
fn packed_sources_bake_sort_and_draw_without_expanded_assets() {
    use bevy::render::{
        RenderApp,
        render_asset::RenderAssets,
        renderer::{RenderDevice, RenderQueue},
        sync_world::RenderEntity,
    };
    use bevy_gaussian_splatting::{
        CloudSettings, Gaussian3d, GaussianRasterProfile, PlanarGaussian3d, PlanarGaussian3dHandle,
        PlanarSync,
        io::{sog, sog_packed},
        render::{
            compact::CompactCloud,
            sog::{
                GpuSogSource,
                work::{BakeRange, CompactBaker, CompactWork, LocalView},
            },
        },
        sort::{SortedEntries, SortedEntriesHandle, radix::RadixSortBuffers},
    };
    let (mut app, target, camera) = render_app(32);
    app.update();
    let view = app
        .world()
        .get::<RenderEntity>(camera)
        .expect("render camera")
        .id();
    let device = app.world().resource::<RenderDevice>().clone();
    let queue = app.world().resource::<RenderQueue>().clone();
    let raw = device.wgpu_device();
    let (meta, files) = chunk_data(Vec3::new(0., 0., -2.), false);
    let packed = sog_packed::decode(
        &sog::parse_meta(&meta).unwrap(),
        &files,
        3,
        sog_packed::DecodeLimits {
            max_texture_dimension: 8192,
            max_texture_bytes: 1024 * 1024,
        },
    )
    .unwrap();
    let source = GpuSogSource::upload(
        raw,
        &queue,
        &GpuSogSource::layout(raw),
        &packed,
        1024 * 1024,
    )
    .unwrap();
    let work = CompactWork::new(raw, 32, 768).unwrap();
    let baker = CompactBaker::new(raw).unwrap();
    let mut encoder = device.create_command_encoder(&default());
    baker
        .encode(
            raw,
            &mut encoder,
            &source,
            &work,
            BakeRange {
                source_start: 0,
                count: 1,
                destination_start: 0,
            },
            LocalView::perspective([0.; 3]).unwrap(),
            false,
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let indirect = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
        label: Some("compact integration indirect"),
        size: 16,
        usage: bevy::render::render_resource::BufferUsages::STORAGE
            | bevy::render::render_resource::BufferUsages::INDIRECT
            | bevy::render::render_resource::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let compact =
        CompactCloud::new(work.buffers().map(|b| b.clone().into()), indirect, 1, view).unwrap();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .reserve_handle();
    let id = handle.id();
    let sorted = app
        .world_mut()
        .resource_mut::<Assets<SortedEntries>>()
        .add(SortedEntries::new(1, 32));
    let entity = app
        .world_mut()
        .spawn((
            compact,
            PlanarGaussian3dHandle(handle),
            SortedEntriesHandle(sorted),
            CloudSettings {
                raster_profile: GaussianRasterProfile::NormalizedGaussian,
                ..default()
            },
            bevy::camera::primitives::Aabb::from_min_max(
                Vec3::new(-1.3, -1., -2.7),
                Vec3::new(1.3, 1., -1.3),
            ),
            Transform::IDENTITY,
            Visibility::Inherited,
        ))
        .id();
    let deadline = Instant::now() + Duration::from_secs(30);
    let pixels = loop {
        let pixels = capture(&mut app, &target);
        if coloured_pixels(&pixels, 0) > 10 {
            break pixels;
        }
        assert!(
            Instant::now() < deadline,
            "compact draw never became visible"
        );
    };
    assert!(
        app.world()
            .resource::<Assets<PlanarGaussian3d>>()
            .get(id)
            .is_none()
    );
    assert!(
        app.sub_app(RenderApp)
            .world()
            .resource::<RenderAssets<<Gaussian3d as PlanarSync>::GpuPlanarType>>()
            .get(id)
            .is_none(),
        "no hidden expanded GPU allocation"
    );
    let original_coverage = coloured_pixels(&pixels, 0);
    app.world_mut()
        .get_mut::<CompactCloud>(entity)
        .unwrap()
        .enable_projection_cache(&device, 32 * 32)
        .expect("bounded projection cache");
    let deadline = Instant::now() + Duration::from_secs(30);
    let projected = loop {
        let image = capture(&mut app, &target);
        let prepared = {
            let render = app.sub_app_mut(RenderApp).world_mut();
            render
                .query::<&CompactCloud>()
                .iter(render)
                .any(|cloud| cloud.has_projection_cache() && cloud.is_prepared())
        };
        if prepared && coloured_pixels(&image, 0) > 10 {
            break image;
        }
        assert!(
            Instant::now() < deadline,
            "compute projection never became ready"
        );
    };
    let maximum_error = pixels
        .iter()
        .zip(&projected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        maximum_error <= 8,
        "cached projection changed pixels by {maximum_error}"
    );
    // A second Gaussian view must not read colours baked for the owner or
    // overwrite its shared depth order. Ordinary UI cameras have no work either.
    let other_target =
        app.world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                64,
                64,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
    let other_camera = app
        .world_mut()
        .spawn((
            Camera3d::default(),
            GaussianCamera::default(),
            MotionVectorPrepass,
            Msaa::Off,
            Tonemapping::None,
            Camera {
                order: 1,
                ..default()
            },
            RenderTarget::Image(other_target.clone().into()),
            Transform::IDENTITY,
        ))
        .id();
    app.update();
    app.update();
    assert!(
        empty_image(&capture(&mut app, &other_target)),
        "compact work is owned by one view"
    );
    assert_eq!(
        coloured_pixels(&capture(&mut app, &target), 0),
        coloured_pixels(&projected, 0)
    );
    app.world_mut().despawn(other_camera);
    // GPU-only work cannot silently fall back to a CPU sorter with no asset.
    app.world_mut()
        .get_mut::<CloudSettings>(entity)
        .unwrap()
        .sort_mode = bevy_gaussian_splatting::sort::SortMode::None;
    assert!(
        empty_image(&capture(&mut app, &target)),
        "unsupported compact sort must not draw stale ordering"
    );
    app.world_mut()
        .get_mut::<CloudSettings>(entity)
        .unwrap()
        .sort_mode = Default::default();
    // Replace the generation with unbaked higher-order sources. The renderer,
    // not the application test, must bake and refresh it as the camera moves.
    let mut sh_meta = sog::parse_meta(&meta).unwrap();
    sh_meta.sh0.codebook = vec![0.0];
    sh_meta.sh_n = Some(sog::SogShN {
        count: 1,
        bands: 1,
        codebook: vec![0., 1.],
        files: vec!["centroids.webp".into(), "labels.webp".into()],
    });
    let mut sh_files = files.clone();
    sh_files.insert("dc.webp".into(), webp([0, 0, 0, 255], 1));
    sh_files.insert("centroids.webp".into(), webp([1, 0, 0, 255], 192));
    sh_files.insert("labels.webp".into(), webp([0, 0, 0, 255], 1));
    let sh_packed = sog_packed::decode(
        &sh_meta,
        &sh_files,
        3,
        sog_packed::DecodeLimits {
            max_texture_dimension: 8192,
            max_texture_bytes: 1024 * 1024,
        },
    )
    .unwrap();
    let sh_source = GpuSogSource::upload(
        raw,
        &queue,
        &GpuSogSource::layout(raw),
        &sh_packed,
        1024 * 1024,
    )
    .unwrap();
    use bevy::render::renderer::WgpuWrapper;
    use bevy_gaussian_splatting::render::sog::view::SourceRange;
    use std::sync::Arc;
    let indirect = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
        label: None,
        size: 16,
        usage: bevy::render::render_resource::BufferUsages::STORAGE
            | bevy::render::render_resource::BufferUsages::INDIRECT
            | bevy::render::render_resource::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let baker = Arc::new(WgpuWrapper::new(baker));
    let sh_source = Arc::new(WgpuWrapper::new(sh_source));
    let retired_source = Arc::downgrade(&sh_source);
    let mut automatic = CompactCloud::from_sources(
        Arc::new(WgpuWrapper::new(CompactWork::new(raw, 32, 768).unwrap())),
        baker.clone(),
        vec![SourceRange {
            source: sh_source,
            range: BakeRange {
                source_start: 0,
                count: 1,
                destination_start: 0,
            },
        }],
        indirect,
        view,
    )
    .unwrap();
    automatic.enable_projection_cache(&device, 32 * 32).unwrap();
    app.world_mut().entity_mut(entity).insert(automatic);
    let mut red = Vec::new();
    for x in [-1., 1., -1.] {
        app.world_mut()
            .entity_mut(camera)
            .insert(Transform::from_xyz(x, 0., 0.).looking_at(Vec3::new(0., 0., -2.), Vec3::Y));
        app.update();
        app.update();
        let image = capture(&mut app, &target);
        let centre = &image[(32 * 64 + 32) * 4..][..4];
        assert!(
            centre[1] > 100,
            "automatic bake must draw neutral green, got {centre:?}"
        );
        red.push(centre[0]);
    }
    assert!(
        red[1] > red[0] + 50,
        "moving camera must re-evaluate SH, got {red:?}"
    );
    assert_eq!(red[0], red[2], "returning camera restores source-space SH");
    // Moving the source and camera together preserves both projection and SH.
    // The inverse scene transform must be applied once, not zero or twice.
    let before_transform = capture(&mut app, &target);
    let scene = Transform::from_xyz(7., 2., -3.).with_rotation(Quat::from_rotation_y(0.7));
    let camera_transform = *app.world().get::<Transform>(camera).unwrap();
    app.world_mut().entity_mut(entity).insert(scene);
    app.world_mut()
        .entity_mut(camera)
        .insert(scene.mul_transform(camera_transform));
    let transformed = capture(&mut app, &target);
    let max_delta = before_transform
        .iter()
        .zip(&transformed)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        max_delta <= 2,
        "source-space transform changed compact pixels by {max_delta}"
    );
    app.world_mut()
        .entity_mut(entity)
        .insert(Transform::IDENTITY);
    // Two independent source files must share one depth sort. Reversing range
    // concatenation must not reverse alpha ordering of their overlapping splats.
    app.world_mut()
        .entity_mut(camera)
        .insert(Transform::IDENTITY);
    let mut source_files = Vec::new();
    for (z, green) in [(-2., false), (-3., true)] {
        let (meta, mut files) = chunk_data(Vec3::new(0., 0., z), green);
        files.insert(
            "dc.webp".into(),
            webp(
                if green {
                    [0, 1, 0, 128]
                } else {
                    [1, 0, 0, 128]
                },
                if green { 2 } else { 1 },
            ),
        );
        let packed = sog_packed::decode(
            &sog::parse_meta(&meta).unwrap(),
            &files,
            3,
            sog_packed::DecodeLimits {
                max_texture_dimension: 8192,
                max_texture_bytes: 1024 * 1024,
            },
        )
        .unwrap();
        source_files.push(Arc::new(WgpuWrapper::new(
            GpuSogSource::upload(
                raw,
                &queue,
                &GpuSogSource::layout(raw),
                &packed,
                1024 * 1024,
            )
            .unwrap(),
        )));
    }
    let mut ordered_pixels = None;
    for compact in [false, true] {
        app.world_mut()
            .get_mut::<CloudSettings>(entity)
            .unwrap()
            .visibility_compaction = compact;
        for order in [[0, 1], [1, 0]] {
            let indirect = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
                label: None,
                size: 16,
                usage: bevy::render::render_resource::BufferUsages::STORAGE
                    | bevy::render::render_resource::BufferUsages::INDIRECT
                    | bevy::render::render_resource::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let sources = order
                .into_iter()
                .enumerate()
                .map(|(destination, file)| SourceRange {
                    source: source_files[file].clone(),
                    range: BakeRange {
                        source_start: 0,
                        count: 1,
                        destination_start: destination as u32,
                    },
                })
                .collect();
            let mut projected_cloud = CompactCloud::from_sources(
                Arc::new(WgpuWrapper::new(CompactWork::new(raw, 32, 768).unwrap())),
                baker.clone(),
                sources,
                indirect,
                view,
            )
            .unwrap();
            projected_cloud
                .enable_projection_cache(&device, 32 * 32)
                .unwrap();
            app.world_mut().entity_mut(entity).insert(projected_cloud);
            // Compaction specialization may compile asynchronously; wait for the
            // requested variant before asserting pixels, never fabricate readiness.
            until(&mut app, "compact sort stages", |app| {
                let render = app.sub_app(RenderApp).world();
                render.resource::<bevy_gaussian_splatting::sort::radix::RadixSortPipeline<Gaussian3d>>().variant_for_projection(Default::default(), true, true).is_some_and(|v| v.is_loaded(render.resource::<bevy::render::render_resource::PipelineCache>(), compact))
            });
            let image = capture(&mut app, &target);
            let centre = &image[(32 * 64 + 32) * 4..][..4];
            assert!(
                centre[0] > centre[1] + 20,
                "near red must sort over far green: {centre:?}"
            );
            if let Some(previous) = &ordered_pixels {
                assert_eq!(
                    &image, previous,
                    "range order/compaction changed raster pixels"
                );
            }
            ordered_pixels = Some(image);
        }
    }
    assert!(
        retired_source.upgrade().is_none(),
        "retired generation must release its source pins"
    );
    app.world_mut()
        .get_mut::<CloudSettings>(entity)
        .unwrap()
        .visibility_compaction = false;
    // Restore the original SH0 generation for the independent FOV check.
    let indirect = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
        label: None,
        size: 16,
        usage: bevy::render::render_resource::BufferUsages::STORAGE
            | bevy::render::render_resource::BufferUsages::INDIRECT
            | bevy::render::render_resource::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut restored =
        CompactCloud::new(work.buffers().map(|b| b.clone().into()), indirect, 1, view).unwrap();
    restored.enable_projection_cache(&device, 32 * 32).unwrap();
    app.world_mut().entity_mut(entity).insert(restored);
    app.world_mut()
        .entity_mut(camera)
        .insert(Transform::IDENTITY);
    let Projection::Perspective(mut projection) =
        app.world().get::<Projection>(camera).unwrap().clone()
    else {
        panic!("perspective")
    };
    projection.fov = std::f32::consts::FRAC_PI_4;
    app.world_mut()
        .entity_mut(camera)
        .insert(Projection::Perspective(projection));
    app.update();
    app.update();
    assert!(
        coloured_pixels(&capture(&mut app, &target), 0) > original_coverage * 2,
        "compact draw must respect live FOV"
    );
    app.world_mut()
        .entity_mut(camera)
        .insert(Transform::from_xyz(100., 0., 0.));
    app.update();
    app.update();
    assert!(
        empty_image(&capture(&mut app, &target)),
        "compact draw must respect live camera position"
    );
    app.world_mut().entity_mut(entity).despawn();
    app.update();
    app.update();
    assert!(
        !app.sub_app(RenderApp)
            .world()
            .resource::<RadixSortBuffers<Gaussian3d>>()
            .asset_map
            .contains_key(&id),
        "compact teardown must retire sort scratch"
    );
}
