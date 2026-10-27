use super::*;
use bevy::ecs::system::SystemState;
use readiness::ReadySnapshots;

fn world() -> World {
    let mut world = World::new();
    world.init_resource::<Assets<PlanarGaussian3d>>();
    world.init_resource::<ReadySnapshots>();
    // These fixtures exercise the explicit CPU fallback without a RenderDevice.
    world.insert_resource(SogRendererBackend::Snapshot);
    world.init_resource::<gpu::GpuResident>();
    world.init_resource::<packed::PackedResident>();
    world.init_resource::<gpu::GpuBudget>();
    world.init_resource::<crate::runtime::SogRuntimeSettings>();
    world.init_resource::<crate::runtime::SogSelectionStatistics>();
    world.init_resource::<gpu::GpuTransfers>();
    world.init_resource::<Assets<bevy_gaussian_splatting::sort::SortedEntries>>();
    world
}

fn chunk() -> DecodedChunk {
    DecodedChunk {
        cloud: vec![Gaussian3d::default()].into(),
        count: 1,
    }
}

#[test]
fn snapshot_publishes_scale_aware_bounds_before_bevy_visibility() {
    let mut world = world();
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    let mut source = chunk();
    source.cloud.expanded_mut().position_visibility[0].position = [20., 2., -30.];
    source.cloud.expanded_mut().scale_opacity[0].scale = [0.1, 2., 0.3];
    let mut state = SnapshotState::default();
    adapter
        .get_mut(&mut world)
        .expect("renderer")
        .submit(
            &mut state,
            scene,
            camera,
            &[SplatRange {
                file: 0,
                offset: 0,
                count: 1,
            }],
            |_| Some(&source),
        )
        .expect("snapshot");
    adapter.apply(&mut world);
    let pending = state.pending.expect("pending entity");
    let bounds = world
        .get::<bevy::camera::primitives::Aabb>(pending)
        .expect("explicit bounds must prevent upstream's centre-only fallback");
    assert_eq!(Vec3::from(bounds.center), Vec3::new(20., 2., -30.));
    assert_eq!(Vec3::from(bounds.half_extents), Vec3::splat(6.));
    assert!(
        world
            .get::<bevy::camera::visibility::NoFrustumCulling>(pending)
            .is_none(),
        "fix the bounds, not disable all cloud culling"
    );
}

#[test]
fn snapshot_submission_records_composition_but_not_gpu_upload_time() {
    use crate::diagnostics::sog_cpu::SogCpuTimings;
    let mut world = world();
    world.insert_resource(SogCpuTimings::enabled());
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    let source = chunk();
    let mut state = SnapshotState::default();
    adapter
        .get_mut(&mut world)
        .expect("renderer")
        .submit(
            &mut state,
            scene,
            camera,
            &[SplatRange {
                file: 0,
                offset: 0,
                count: 1,
            }],
            |_| Some(&source),
        )
        .expect("snapshot");
    let stages = world
        .resource::<SogCpuTimings>()
        .report()
        .cpu_stages
        .expect("enabled");
    assert_eq!(stages.snapshot_compose.attempts, 1);
    assert!(stages.snapshot_compose.total_ms.is_some());
    assert_eq!(stages.source_upload_prepare.attempts, 0);
    assert!(stages.source_upload_prepare.total_ms.is_none());
}

#[test]
fn snapshot_obeys_two_generation_slot_memory_cap() {
    let mut world = world();
    world
        .resource_mut::<gpu::GpuBudget>()
        .selected_bytes_per_scene = 0;
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    assert_eq!(adapter.get_mut(&mut world).unwrap().splat_limit(), 0);
}

#[test]
fn snapshot_capacity_uses_live_gaussian_marker_count_not_ui_camera_count() {
    let mut world = world();
    world.spawn(Camera2d);
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    let initial = adapter.get_mut(&mut world).unwrap().splat_limit();
    // Marker count is exactly the upstream sort contract, even without Camera.
    for _ in 0..100 {
        world.spawn(GaussianCamera::default());
    }
    let constrained = adapter.get_mut(&mut world).unwrap().splat_limit();
    assert!(constrained > 0 && constrained < initial);
}

#[test]
fn concurrent_submission_cannot_orphan_a_pending_snapshot() {
    let mut world = world();
    let root = world.spawn_empty().id();
    let camera = world
        .spawn(bevy::core_pipeline::tonemapping::Tonemapping::Reinhard)
        .id();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    let mut state = SnapshotState::default();
    let chunk = chunk();
    let ranges = [SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    }];
    {
        let mut renderer = adapter.get_mut(&mut world).unwrap();
        renderer
            .submit(&mut state, root, camera, &ranges, |_| Some(&chunk))
            .unwrap();
        let pending = state.pending;
        assert!(
            renderer
                .submit(&mut state, root, camera, &ranges, |_| Some(&chunk))
                .is_err()
        );
        assert_eq!(state.pending, pending);
    }
    adapter.apply(&mut world);
    assert_eq!(
        world.get::<bevy::core_pipeline::tonemapping::Tonemapping>(camera),
        Some(&bevy::core_pipeline::tonemapping::Tonemapping::Reinhard),
        "streaming must not steal appearance ownership"
    );
    assert_eq!(world.get::<Children>(root).unwrap().len(), 1);
}

#[test]
fn another_scenes_readiness_cannot_replace_the_visible_snapshot() {
    let mut world = world();
    let old = world.spawn(Visibility::Inherited).id();
    let pending = world.spawn(Visibility::Hidden).id();
    let unrelated = world.spawn(Visibility::Hidden).id();
    let mut state = SnapshotState {
        current: Some(old),
        pending: Some(pending),
        ..default()
    };
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(unrelated);
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    assert!(!adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    assert!(world.get_entity(old).is_ok());
    assert_eq!(
        *world.get::<Visibility>(pending).unwrap(),
        Visibility::Hidden
    );
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    assert!(world.get_entity(old).is_err());
    assert_eq!(
        *world.get::<Visibility>(pending).unwrap(),
        Visibility::Inherited
    );
    assert!(state.is_visible());
    assert!(!state.is_pending());
}

#[test]
fn rejects_oversized_or_empty_selections_before_copying_sources() {
    let mut world = world();
    let root = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    for ranges in [
        vec![],
        vec![SplatRange {
            file: 0,
            offset: 0,
            count: usize::MAX,
        }],
    ] {
        let mut state = SnapshotState::default();
        let result =
            adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut state, root, camera, &ranges, |_| {
                    panic!("must reject before reading sources")
                });
        assert!(result.is_err());
        assert!(!state.is_pending());
    }
    adapter.apply(&mut world);
    assert!(world.get::<Children>(root).is_none());
    assert!(world.resource::<Assets<PlanarGaussian3d>>().is_empty());
}

#[test]
fn empty_selection_clear_retires_current_and_pending() {
    let mut world = world();
    let old = world.spawn(Visibility::Inherited).id();
    let pending = world.spawn(Visibility::Hidden).id();
    let mut state = SnapshotState::default();
    state.current = Some(old);
    state.pending = Some(pending);
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    adapter.get_mut(&mut world).unwrap().clear(&mut state);
    adapter.apply(&mut world);
    assert!(!state.is_visible());
    assert!(!state.is_pending());
    assert!(world.get_entity(old).is_err());
    assert!(world.get_entity(pending).is_err());
}

#[test]
fn checked_gpu_ranges_reject_malformed_planes_and_overflow() {
    let chunk = chunk();
    assert!(
        gpu::SelectionPlan::new(
            &[SplatRange {
                file: 0,
                offset: usize::MAX,
                count: 1
            }],
            |_| Some(&chunk),
            32
        )
        .is_err()
    );
    let malformed = DecodedChunk {
        cloud: PlanarGaussian3d::default().into(),
        count: 1,
    };
    assert!(
        gpu::SelectionPlan::new(
            &[SplatRange {
                file: 0,
                offset: 0,
                count: 1
            }],
            |_| Some(&malformed),
            32
        )
        .is_err()
    );
    let plan = gpu::SelectionPlan::new(
        &[SplatRange {
            file: 0,
            offset: 0,
            count: 1,
        }],
        |_| Some(&chunk),
        32,
    )
    .unwrap();
    assert_eq!(plan.count, 1);
    assert_eq!(plan.padded, 32);
}

#[test]
fn gpu_slot_budget_includes_two_slots_and_radix_scratch() {
    assert_eq!(gpu::selected_limit(0, 1024), 0);
    let limit = gpu::selected_limit(128 * 1024, 1_000_000);
    assert!(limit > 0 && limit < 1024);
    assert_eq!(limit % 32, 0);
    assert_eq!(gpu::selected_limit(u64::MAX, 1024), 1024);
}

#[test]
fn backend_switch_cannot_reassign_live_state() {
    let mut world = world();
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    let current = world.spawn_empty().id();
    let mut state = SnapshotState {
        current: Some(current),
        pending: None,
        owner: Some((SogRendererBackend::Snapshot, scene)),
        ..default()
    };
    world.insert_resource(SogRendererBackend::GpuResident);
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    let ranges = [SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    }];
    assert!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &ranges, |_| panic!(
                "must reject before source access"
            ))
            .is_err()
    );
    assert_eq!(state.current, Some(current));
    assert!(!state.is_pending());
}

#[test]
fn snapshot_compaction_setting_and_logical_submission_counters_survive_handoff() {
    use crate::runtime::{SogRuntimeSettings, SogSelectionStatistics};
    let mut world = world();
    world.resource_mut::<SogRuntimeSettings>().compaction = true;
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    let chunk = chunk();
    let mut state = SnapshotState::default();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    adapter
        .get_mut(&mut world)
        .unwrap()
        .submit(
            &mut state,
            scene,
            camera,
            &[SplatRange {
                file: 0,
                offset: 0,
                count: 1,
            }],
            |_| Some(&chunk),
        )
        .unwrap();
    adapter.apply(&mut world);
    let pending = state.pending.unwrap();
    assert_eq!(
        serde_json::to_value(world.get::<CloudSettings>(pending).unwrap()).unwrap()["raster_profile"],
        "NormalizedGaussian"
    );
    assert!(
        world
            .get::<CloudSettings>(pending)
            .unwrap()
            .visibility_compaction
    );
    assert_eq!(world.resource::<SogSelectionStatistics>().counts[&scene], 1);
    assert_eq!(
        world
            .resource::<SogSelectionStatistics>()
            .submitted_generations,
        1
    );
    assert_eq!(
        world.resource::<SogSelectionStatistics>().ready_generations,
        0
    );
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    assert_eq!(
        world.resource::<SogSelectionStatistics>().ready_generations,
        1
    );
    let settings = world.get::<CloudSettings>(state.current.unwrap()).unwrap();
    assert_eq!(
        settings.raster_profile,
        GaussianRasterProfile::NormalizedGaussian
    );
    world.resource_mut::<SogRuntimeSettings>().compaction = false;
    adapter
        .get_mut(&mut world)
        .unwrap()
        .submit(
            &mut state,
            scene,
            camera,
            &[SplatRange {
                file: 0,
                offset: 0,
                count: 1,
            }],
            |_| Some(&chunk),
        )
        .unwrap();
    adapter.apply(&mut world);
    let next = state.pending.unwrap();
    let settings = world.get::<CloudSettings>(next).unwrap();
    assert_eq!(
        settings.raster_profile,
        GaussianRasterProfile::NormalizedGaussian
    );
    assert!(!settings.visibility_compaction);
    // The previous visible generation keeps its profile while the new one is pending.
    assert_eq!(
        world.get::<CloudSettings>(pending).unwrap().raster_profile,
        GaussianRasterProfile::NormalizedGaussian
    );
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(next);
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    assert_eq!(
        world
            .get::<CloudSettings>(state.current.unwrap())
            .unwrap()
            .raster_profile,
        GaussianRasterProfile::NormalizedGaussian
    );
    adapter.get_mut(&mut world).unwrap().clear(&mut state);
    adapter.apply(&mut world);
    assert!(world.resource::<SogSelectionStatistics>().counts.is_empty());
    assert!(world.get_entity(pending).is_err());
    assert!(world.get_entity(next).is_err());
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn packed_eviction_preserves_the_only_copy_when_a_recoverable_victim_fits() {
    packed_recovery_eviction_case(false, true, 2, true, false);
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn packed_eviction_observes_cpu_backup_loss_after_upload() {
    packed_recovery_eviction_case(false, false, 2, false, true);
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn packed_eviction_keeps_lru_order_when_both_sources_are_recoverable() {
    packed_recovery_eviction_case(true, true, 2, false, true);
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn packed_eviction_can_drop_the_only_copy_when_recoverable_space_is_insufficient() {
    packed_recovery_eviction_case(false, true, 1, false, false);
}

fn packed_recovery_eviction_case(
    keep_old_cpu: bool,
    keep_new_cpu: bool,
    cap_slots: u64,
    old_survives: bool,
    new_survives: bool,
) {
    use bevy_gaussian_splatting::io::sog_packed::{DataTexture, DecodeLimits, decode_rgba};
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    let mut world = world();
    world.insert_resource(resources.0);
    world.insert_resource(resources.1);
    world.insert_resource(SogRendererBackend::Packed);
    world.init_resource::<bevy::diagnostic::FrameCount>();
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    world
        .entity_mut(camera)
        .insert(bevy::render::sync_world::RenderEntity::from(camera));
    let mut json: serde_json::Value = serde_json::from_str(include_str!(
        "../streaming/runtime/fixtures/worker-expanded-v2.json"
    ))
    .unwrap();
    json["meta"].as_object_mut().unwrap().remove("shN");
    let meta = serde_json::from_value(json["meta"].clone()).unwrap();
    let textures = [
        "lo.webp",
        "hi.webp",
        "quats.webp",
        "scales.webp",
        "sh0.webp",
    ]
    .into_iter()
    .map(|name| {
        let mut bytes = vec![0; 8];
        if name == "quats.webp" {
            bytes[3] = 252;
            bytes[7] = 252;
        }
        (
            name.to_string(),
            DataTexture::from_rgba(2, 1, bytes).unwrap(),
        )
    })
    .collect();
    let mut chunks: std::collections::HashMap<_, _> = [4, 26, 25]
        .into_iter()
        .map(|file| {
            let data = decode_rgba(
                &meta,
                &textures,
                0,
                DecodeLimits {
                    max_texture_dimension: 4096,
                    max_texture_bytes: 1024,
                },
            )
            .unwrap();
            (
                file,
                DecodedChunk {
                    count: 2,
                    cloud: source::SourceData::Packed(data.into()),
                },
            )
        })
        .collect();
    let bytes = bevy_gaussian_splatting::render::sog::GpuSogSource::required_bytes(
        chunks[&4].cloud.packed().unwrap(),
    );
    world.resource_mut::<gpu::GpuBudget>().source_bytes = 2 * bytes;
    let mut state = SnapshotState::default();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    // Replay the important ownership transitions, not wall-clock delays:
    // old file 4 loses its CPU copy; newer file 26 remains recoverable.
    for file in [4, 26, 25] {
        if file == 25 {
            world.resource_mut::<gpu::GpuBudget>().source_bytes = cap_slots * bytes;
        }
        for _ in 0..32 {
            world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
            let result = adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(
                    &mut state,
                    scene,
                    camera,
                    &[SplatRange {
                        file,
                        offset: 0,
                        count: 2,
                    }],
                    |id| chunks.get(&id),
                )
                .unwrap();
            adapter.apply(&mut world);
            assert!(
                world
                    .resource::<packed::PackedResident>()
                    .statistics
                    .resident_bytes
                    <= world.resource::<gpu::GpuBudget>().source_bytes
            );
            if result == Submission::Submitted {
                break;
            }
        }
        let pending = state.pending.expect("bounded upload completes");
        world
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .insert(pending);
        assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
        adapter.apply(&mut world);
        // Simulate acknowledged retirement, leaving sources evictable but cached.
        world.despawn(state.current.take().unwrap());
        if (file == 4 && !keep_old_cpu) || (file == 26 && !keep_new_cpu) {
            chunks.remove(&file);
        }
    }
    let resident = world.resource::<packed::PackedResident>();
    let mut old = None;
    resident.sync_source(scene, 4, &mut old);
    assert_eq!(
        old.is_some(),
        old_survives,
        "file 4 GPU residency after pressure"
    );
    let mut recoverable = None;
    resident.sync_source(scene, 26, &mut recoverable);
    assert_eq!(
        recoverable.is_some(),
        new_survives,
        "file 26 GPU residency after pressure"
    );
    if keep_new_cpu {
        assert!(
            chunks[&26].cloud.packed().is_ok(),
            "eviction must not discard CPU recovery"
        );
    }
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn competing_scene_cannot_evict_completed_sources_of_a_preparing_selection() {
    use bevy_gaussian_splatting::io::sog_packed::{DataTexture, DecodeLimits, decode_rgba};
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    let mut world = world();
    world.insert_resource(resources.0);
    world.insert_resource(resources.1);
    world.insert_resource(SogRendererBackend::Packed);
    world.init_resource::<bevy::diagnostic::FrameCount>();
    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    world
        .entity_mut(camera)
        .insert(bevy::render::sync_world::RenderEntity::from(camera));
    let chunk = |width: u32, height: u32| {
        let mut json: serde_json::Value = serde_json::from_str(include_str!(
            "../streaming/runtime/fixtures/worker-expanded-v2.json"
        ))
        .unwrap();
        json["meta"].as_object_mut().unwrap().remove("shN");
        json["meta"]["count"] = (width * height).into();
        let meta = serde_json::from_value(json["meta"].clone()).unwrap();
        let textures = [
            "lo.webp",
            "hi.webp",
            "quats.webp",
            "scales.webp",
            "sh0.webp",
        ]
        .into_iter()
        .map(|name| {
            let mut bytes = vec![0; (width * height * 4) as usize];
            if name == "quats.webp" {
                for pixel in bytes.chunks_exact_mut(4) {
                    pixel[3] = 252;
                }
            }
            (
                name.to_owned(),
                DataTexture::from_rgba(width, height, bytes).unwrap(),
            )
        })
        .collect();
        let packed = decode_rgba(
            &meta,
            &textures,
            0,
            DecodeLimits {
                max_texture_dimension: 4096,
                max_texture_bytes: 12 << 20,
            },
        )
        .unwrap();
        DecodedChunk {
            count: (width * height) as usize,
            cloud: source::SourceData::Packed(packed.into()),
        }
    };
    let small = chunk(2, 1);
    let large = chunk(1024, 512);
    let bytes = |chunk: &DecodedChunk| {
        bevy_gaussian_splatting::render::sog::GpuSogSource::required_bytes(
            chunk.cloud.packed().unwrap(),
        )
    };
    let cap = bytes(&small) + bytes(&large);
    world.resource_mut::<gpu::GpuBudget>().source_bytes = cap;
    let ranges = [
        SplatRange {
            file: 0,
            offset: 0,
            count: 2,
        },
        SplatRange {
            file: 1,
            offset: 0,
            count: 2,
        },
    ];
    for end in ["clear", "invalid range", "empty selection", "supersede"] {
        let mut first_state = SnapshotState::default();
        let mut second_state = SnapshotState::default();
        let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
        let mut descriptor = None;
        for _ in 0..32 {
            world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
            let result = adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut first_state, first, camera, &ranges, |file| {
                    Some(if file == 0 { &small } else { &large })
                })
                .unwrap();
            adapter.apply(&mut world);
            let resident = world.resource::<packed::PackedResident>();
            resident.sync_source(first, 0, &mut descriptor);
            if descriptor.is_some() && resident.scene_source_bytes(first) == cap {
                assert_eq!(
                    result,
                    Submission::Preparing,
                    "large source spans several upload frames"
                );
                break;
            }
        }
        assert!(descriptor.is_some(), "first source has finished uploading");
        assert_eq!(
            world
                .resource::<packed::PackedResident>()
                .scene_source_bytes(first),
            cap
        );
        assert!(
            !first_state.is_pending(),
            "no draw generation owns the completed source yet"
        );
        let uploaded = world
            .resource::<packed::PackedResident>()
            .statistics
            .uploaded_bytes;
        // The partial large upload is already protected; the completed small source
        // must be protected too, until a generation takes ownership or we cancel.
        let result = adapter.get_mut(&mut world).unwrap().submit(
            &mut second_state,
            second,
            camera,
            &ranges[..1],
            |_| Some(&small),
        );
        assert!(
            result.is_err(),
            "competing scene stole an in-preparation source"
        );
        assert_eq!(
            world
                .resource::<packed::PackedResident>()
                .statistics
                .uploaded_bytes,
            uploaded
        );
        assert!(
            !world
                .resource::<packed::PackedResident>()
                .sync_source(first, 0, &mut descriptor)
        );
        assert!(descriptor.is_some());
        assert_eq!(
            adapter
                .get_mut(&mut world)
                .unwrap()
                .source_cache_bytes(second),
            Some(0)
        );
        match end {
            "clear" => adapter.get_mut(&mut world).unwrap().clear(&mut first_state),
            "supersede" => {
                assert_eq!(
                    adapter
                        .get_mut(&mut world)
                        .unwrap()
                        .submit(&mut first_state, first, camera, &ranges[..1], |_| Some(
                            &small
                        ),)
                        .unwrap(),
                    Submission::Submitted
                );
            }
            _ => {
                let invalid = [SplatRange {
                    count: 3,
                    ..ranges[0]
                }];
                let candidate = if end == "empty selection" {
                    &[][..]
                } else {
                    &invalid[..]
                };
                assert!(
                    adapter
                        .get_mut(&mut world)
                        .unwrap()
                        .submit(&mut first_state, first, camera, candidate, |_| Some(&small),)
                        .is_err()
                );
            }
        }
        adapter.apply(&mut world);
        assert_eq!(
            adapter
                .get_mut(&mut world)
                .unwrap()
                .source_cache_bytes(second),
            Some(if end == "supersede" {
                cap - bytes(&small)
            } else {
                cap
            }),
            "{end} must release only preparation ownership"
        );
        for _ in 0..16 {
            world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
            if adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut second_state, second, camera, &ranges[..1], |_| {
                    Some(&small)
                })
                .unwrap()
                == Submission::Submitted
            {
                break;
            }
        }
        adapter.apply(&mut world);
        assert!(
            second_state.is_pending(),
            "cancellation must release preparation ownership"
        );
        assert!(
            world
                .resource::<packed::PackedResident>()
                .statistics
                .resident_bytes
                <= cap
        );
        adapter
            .get_mut(&mut world)
            .unwrap()
            .clear(&mut second_state);
        adapter.get_mut(&mut world).unwrap().clear(&mut first_state);
        adapter.apply(&mut world);
    }
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn partial_packed_upload_is_bounded_charged_cancelable_and_never_visible() {
    use bevy_gaussian_splatting::io::sog_packed::{DataTexture, DecodeLimits, decode_rgba};
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    let mut world = world();
    world.insert_resource(resources.0);
    world.insert_resource(resources.1);
    world.insert_resource(SogRendererBackend::Packed);
    world.init_resource::<bevy::diagnostic::FrameCount>();
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    world
        .entity_mut(camera)
        .insert(bevy::render::sync_world::RenderEntity::from(camera));
    let mut meta: serde_json::Value = serde_json::from_str(include_str!(
        "../streaming/runtime/fixtures/worker-expanded-v2.json"
    ))
    .unwrap();
    meta["meta"].as_object_mut().unwrap().remove("shN");
    meta["meta"]["count"] = 524288.into();
    let meta = serde_json::from_value(meta["meta"].clone()).unwrap();
    let textures = [
        "lo.webp",
        "hi.webp",
        "quats.webp",
        "scales.webp",
        "sh0.webp",
    ]
    .into_iter()
    .map(|name| {
        let mut bytes = vec![0; 1024 * 512 * 4];
        if name == "quats.webp" {
            for pixel in bytes.chunks_exact_mut(4) {
                pixel[3] = 252;
            }
        }
        (
            name.to_string(),
            DataTexture::from_rgba(1024, 512, bytes).unwrap(),
        )
    })
    .collect();
    let packed = decode_rgba(
        &meta,
        &textures,
        0,
        DecodeLimits {
            max_texture_dimension: 4096,
            max_texture_bytes: 12 * 1024 * 1024,
        },
    )
    .unwrap();
    let required = bevy_gaussian_splatting::render::sog::GpuSogSource::required_bytes(&packed);
    let chunk = DecodedChunk {
        cloud: source::SourceData::Packed(packed.into()),
        count: 524288,
    };
    let ranges = [SplatRange {
        file: 0,
        offset: 0,
        count: 2,
    }];
    let mut state = SnapshotState::default();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    world.resource_mut::<gpu::GpuBudget>().source_bytes = required - 1;
    assert!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
            .is_err()
    );
    assert_eq!(
        world
            .resource::<packed::PackedResident>()
            .statistics
            .resident_bytes,
        0
    );
    world.resource_mut::<gpu::GpuBudget>().source_bytes = required;
    for cancel in [true, false] {
        world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
        assert_eq!(
            adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
                .unwrap(),
            Submission::Preparing
        );
        adapter.apply(&mut world);
        assert!(!state.is_pending() && !state.is_visible());
        assert!(world.get::<Children>(scene).is_none());
        let packed = world.resource::<packed::PackedResident>();
        assert_eq!(
            packed.statistics.resident_bytes, required,
            "full reservation before publication"
        );
        assert_eq!(packed.scene_source_bytes(scene), required);
        let uploaded = packed.statistics.uploaded_bytes;
        // Retrying within a frame cannot reset the shared allowance.
        assert_eq!(
            adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
                .unwrap(),
            Submission::Preparing
        );
        assert_eq!(
            world
                .resource::<packed::PackedResident>()
                .statistics
                .uploaded_bytes,
            uploaded
        );
        if cancel {
            adapter.get_mut(&mut world).unwrap().clear(&mut state);
            adapter.apply(&mut world);
            assert_eq!(
                world
                    .resource::<packed::PackedResident>()
                    .statistics
                    .resident_bytes,
                0
            );
        }
    }
    for _ in 0..64 {
        world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
        let before = world
            .resource::<packed::PackedResident>()
            .statistics
            .uploaded_bytes;
        let result = adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
            .unwrap();
        let bytes = world
            .resource::<packed::PackedResident>()
            .statistics
            .uploaded_bytes
            - before;
        assert!(bytes <= 4 * 1024 * 1024);
        if result == Submission::Submitted {
            break;
        }
    }
    adapter.apply(&mut world);
    let pending = state.pending.expect("bounded upload eventually completes");
    assert_eq!(world.get::<Visibility>(pending), Some(&Visibility::Hidden));
    assert!(!adapter.get_mut(&mut world).unwrap().poll(&mut state));
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    assert_eq!(
        world.get::<Visibility>(pending),
        Some(&Visibility::Inherited)
    );
    // Render extraction may retain an older slot after main-world retirement.
    // That is pending preparation, not a terminal error that permits idle sleep.
    let retiring = world
        .get::<bevy_gaussian_splatting::render::compact::CompactCloud>(pending)
        .unwrap()
        .clone();
    assert_eq!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
            .unwrap(),
        Submission::Submitted
    );
    adapter.apply(&mut world);
    let second = state.pending.unwrap();
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(second);
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    assert_eq!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
            .unwrap(),
        Submission::Preparing
    );
    assert_eq!(
        world.get::<Visibility>(second),
        Some(&Visibility::Inherited)
    );
    assert!(!state.is_pending());
    drop(retiring);
    assert_eq!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
            .unwrap(),
        Submission::Submitted
    );
}

#[test]
#[ignore = "requires native GPU; no window is opened"]
fn packed_pages_share_palette_admission_uploads_and_retirement() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy_gaussian_splatting::io::sog_packed::{DataTexture, DecodeLimits, decode_rgba};
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    let mut world = world();
    let cache_trace = crate::diagnostics::sog_cache::SogCacheTrace::enabled();
    world.insert_resource(cache_trace.clone());
    world.insert_resource(resources.0);
    world.insert_resource(resources.1);
    world.insert_resource(SogRendererBackend::Packed);
    world.init_resource::<bevy::diagnostic::FrameCount>();
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    world
        .entity_mut(camera)
        .insert(bevy::render::sync_world::RenderEntity::from(camera));
    let mut json: serde_json::Value = serde_json::from_str(include_str!(
        "../streaming/runtime/fixtures/worker-expanded-v2.json"
    ))
    .unwrap();
    json["meta"]["count"] = 4.into();
    let meta = serde_json::from_value(json["meta"].clone()).unwrap();
    let textures = [
        "lo.webp",
        "hi.webp",
        "quats.webp",
        "scales.webp",
        "sh0.webp",
        "labels.webp",
        "centroids.webp",
    ]
    .into_iter()
    .map(|name| {
        let (w, h) = if name == "centroids.webp" {
            (960, 1)
        } else {
            (2, 2)
        };
        let mut bytes = vec![0; w * h * 4];
        if name == "quats.webp" {
            for px in bytes.chunks_exact_mut(4) {
                px[3] = 252;
            }
        }
        (
            name.to_string(),
            DataTexture::from_rgba(w as u32, h as u32, bytes).unwrap(),
        )
    })
    .collect();
    let limits = DecodeLimits {
        max_texture_dimension: 4096,
        max_texture_bytes: 1 << 20,
    };
    let original = decode_rgba(&meta, &textures, 3, limits).unwrap();
    let pages: Vec<_> = (0..3)
        .map(|i| DecodedChunk {
            count: 1,
            cloud: source::SourceData::PackedPage {
                original_file: 7,
                source: original.copy_range(i..i + 1, limits).unwrap(),
            },
        })
        .collect();
    let palette = 3840 + 4096;
    let required = palette + 2 * (24 + 48);
    world.resource_mut::<gpu::GpuBudget>().source_bytes = required;
    let ranges = [
        SplatRange {
            file: 0,
            offset: 0,
            count: 1,
        },
        SplatRange {
            file: 1,
            offset: 0,
            count: 1,
        },
    ];
    let mut state = SnapshotState::default();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    for range_count in [1, 2] {
        for _ in 0..16 {
            world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
            let result = adapter
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut state, scene, camera, &ranges[..range_count], |i| {
                    pages.get(i)
                })
                .unwrap();
            adapter.apply(&mut world);
            if result == Submission::Submitted {
                break;
            }
        }
        let pending = state.pending.expect("page generation completes");
        assert!(!adapter.get_mut(&mut world).unwrap().poll(&mut state));
        world
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .insert(pending);
        assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
        adapter.apply(&mut world);
    }
    let stats = world.resource::<packed::PackedResident>().statistics;
    assert_eq!(stats.resident_bytes, required, "one shared GPU palette");
    assert_eq!(
        stats.uploaded_bytes, required,
        "palette must be uploaded once"
    );
    assert_eq!(
        world
            .resource::<packed::PackedResident>()
            .scene_source_bytes(scene),
        required
    );
    let third = [SplatRange {
        file: 2,
        offset: 0,
        count: 1,
    }];
    assert!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &third, |i| pages.get(i),)
            .is_err(),
        "displayed pages remain pinned at the exact cap"
    );
    // Retire page 0, keeping page 1 visible. The next admission may evict only
    // the unowned page, and must report live owners rather than old CPU intent.
    adapter
        .get_mut(&mut world)
        .unwrap()
        .submit(&mut state, scene, camera, &ranges[1..], |i| pages.get(i))
        .unwrap();
    adapter.apply(&mut world);
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(state.pending.unwrap());
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    for _ in 0..16 {
        world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
        if adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &third, |i| pages.get(i))
            .unwrap()
            == Submission::Submitted
        {
            break;
        }
    }
    adapter.apply(&mut world);
    let report = serde_json::to_value(cache_trace.report().unwrap()).unwrap();
    let eviction = report["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "gpu_admission_eviction")
        .expect("retired page must yield to the third page");
    assert_eq!(eviction["source"], 0);
    let admission = &eviction["gpu_admission"];
    assert_eq!(
        admission["budget_bytes"], 8080,
        "must expose the live GPU cap"
    );
    assert_eq!(
        admission["needed_bytes_before"], 8152,
        "shared palette charged once"
    );
    assert_eq!(admission["resident_bytes_before"], 8080);
    assert_eq!(admission["requested"], serde_json::json!([2]));
    let sources = admission["sources"]
        .as_array()
        .expect("live ownership snapshot");
    let victim = sources.iter().find(|source| source["source"] == 0).unwrap();
    let visible = sources.iter().find(|source| source["source"] == 1).unwrap();
    assert_eq!(victim["external_owners"], 0);
    assert_eq!(victim["evictable"], true);
    assert!(visible["external_owners"].as_u64().unwrap() > 0);
    assert_eq!(visible["evictable"], false);
    assert_eq!(admission["complete"], true);
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(state.pending.unwrap());
    assert!(adapter.get_mut(&mut world).unwrap().poll(&mut state));
    adapter.apply(&mut world);
    let retiring = world
        .get::<bevy_gaussian_splatting::render::compact::CompactCloud>(state.current.unwrap())
        .unwrap()
        .clone();
    adapter.get_mut(&mut world).unwrap().clear(&mut state);
    adapter.apply(&mut world);
    world.run_system_once(packed::cleanup).unwrap();
    assert_eq!(
        world
            .resource::<packed::PackedResident>()
            .statistics
            .resident_bytes,
        palette + 72,
        "only the extracted third page owns the shared palette after clear"
    );
    drop(retiring);
    world.run_system_once(packed::cleanup).unwrap();
    assert_eq!(
        world
            .resource::<packed::PackedResident>()
            .statistics
            .resident_bytes,
        0
    );
}

#[test]
#[ignore = "requires native GPU; supersedes a partial upload without opening a window"]
fn superseded_page_selection_finishes_palette_owner_before_new_pages() {
    use bevy_gaussian_splatting::io::sog_packed::{DataTexture, DecodeLimits, decode_rgba};
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    let mut world = world();
    world.insert_resource(resources.0);
    world.insert_resource(resources.1);
    world.insert_resource(SogRendererBackend::Packed);
    world.init_resource::<bevy::diagnostic::FrameCount>();
    let scene = world.spawn_empty().id();
    let camera = world.spawn_empty().id();
    world
        .entity_mut(camera)
        .insert(bevy::render::sync_world::RenderEntity::from(camera));
    let mut json: serde_json::Value = serde_json::from_str(include_str!(
        "../streaming/runtime/fixtures/worker-expanded-v2.json"
    ))
    .unwrap();
    json["meta"].as_object_mut().unwrap().remove("shN");
    json["meta"]["count"] = 524288.into();
    let meta = serde_json::from_value(json["meta"].clone()).unwrap();
    let textures = [
        "lo.webp",
        "hi.webp",
        "quats.webp",
        "scales.webp",
        "sh0.webp",
    ]
    .into_iter()
    .map(|name| {
        let mut bytes = vec![0; 1024 * 512 * 4];
        if name == "quats.webp" {
            for px in bytes.chunks_exact_mut(4) {
                px[3] = 252;
            }
        }
        (
            name.to_string(),
            DataTexture::from_rgba(1024, 512, bytes).unwrap(),
        )
    })
    .collect();
    let source = decode_rgba(
        &meta,
        &textures,
        0,
        DecodeLimits {
            max_texture_dimension: 4096,
            max_texture_bytes: 12 << 20,
        },
    )
    .unwrap();
    let required = bevy_gaussian_splatting::render::sog::GpuSogSource::required_bytes(&source);
    let chunk = DecodedChunk {
        count: 524288,
        cloud: source::SourceData::PackedPage {
            original_file: 7,
            source,
        },
    };
    let cap = 2 * required - 4100; // SH0 lookup + one fallback centroid, shared once.
    world.resource_mut::<gpu::GpuBudget>().source_bytes = cap;
    let mut state = SnapshotState::default();
    let mut adapter = SystemState::<SnapshotRenderer>::new(&mut world);
    let initial = [SplatRange {
        file: 1,
        offset: 0,
        count: 1,
    }];
    assert_eq!(
        adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &initial, |_| Some(&chunk))
            .unwrap(),
        Submission::Preparing
    );
    adapter.apply(&mut world);
    let replacement = [
        SplatRange {
            file: 0,
            offset: 0,
            count: 1,
        },
        initial[0],
    ];
    for _ in 0..64 {
        world.resource_mut::<bevy::diagnostic::FrameCount>().0 += 1;
        let result = adapter
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &replacement, |_| Some(&chunk))
            .unwrap();
        adapter.apply(&mut world);
        if result == Submission::Submitted {
            break;
        }
    }
    assert!(state.is_pending(), "replacement completes");
    let stats = world.resource::<packed::PackedResident>().statistics;
    assert_eq!(
        stats.uploaded_bytes, cap,
        "supersession cannot duplicate the shared palette"
    );
    assert_eq!(stats.resident_bytes, cap);
    adapter.get_mut(&mut world).unwrap().clear(&mut state);
    adapter.apply(&mut world);
}
