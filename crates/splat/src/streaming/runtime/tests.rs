use super::*;
use crate::renderer::{compose, gpu_splat_limit, readiness::ReadySnapshots};
use crate::streaming::SogLodPolicy;
use bevy_gaussian_splatting::{Gaussian3d, Planar, PlanarGaussian3d};
use std::{collections::HashMap, time::Duration};

mod loading_progress;
mod pressure_lifecycle;

const SCENE: &[u8] = include_bytes!("fixtures/sanatorium-lod.json");

fn pixel_stream() -> SogStream {
    let mut stream = SogStream::parse(
        br#"{"lodLevels":6,"filenames":["fine/meta.json","middle/meta.json","coarse/meta.json"],
        "tree":{"bound":{"min":[-1,-1,-6],"max":[1,1,-4]},"lods":{
        "0":{"file":0,"offset":0,"count":10000},
        "2":{"file":1,"offset":0,"count":2500},
        "5":{"file":2,"offset":0,"count":625}}}}"#,
        "https://example.test/lod-meta.json",
        "pixels".into(),
        "pixels".into(),
    )
    .expect("valid fixture");
    for (entry, count) in stream.entries.iter_mut().zip(&stream.manifest.file_counts) {
        entry.ready = Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); *count].into(),
            count: *count,
        });
    }
    let fine = stream.manifest.leaves[0].lods[0].1;
    stream.active = vec![fine];
    stream.submitted = vec![fine];
    stream.displayed = vec![fine];
    stream
}

#[test]
fn default_policy_stops_refining_small_regions_without_filling_the_budget() {
    let mut stream = pixel_stream();
    let settings = SogLodSettings {
        splat_budget: 100_000,
        ..default()
    };
    let view = selection::View {
        pixels: Some(screen_space::PixelView {
            clip_from_local: Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 1000.),
            viewport: Vec2::splat(200.),
        }),
        ..Vec3::ZERO.into()
    };
    stream.tick(view, 0., &settings, 100_000, None, |_, _, _, _| {
        panic!("cached")
    });
    assert_eq!(stream.active, [stream.manifest.leaves[0].lods[2].1]);
    assert_eq!(
        stream.active.iter().map(|range| range.count).sum::<usize>(),
        625
    );
}

#[test]
fn screen_space_frustum_keeps_render_ready_coarse_coverage_on_turn_and_return() {
    use crate::streaming::SogLodPolicy;
    for pending in [false, true] {
        let (mut app, root, camera) = stream_app(SogLodSettings {
            lod_policy: SogLodPolicy::ScreenSpace,
            visibility: SogVisibilityPolicy::All,
            ..default()
        });
        let old = activate_snapshot(&mut app, root);
        if pending {
            app.world_mut()
                .get_mut::<SogStream>(root)
                .unwrap()
                .submitted
                .clear();
            advance_stream(&mut app);
            assert!(
                app.world()
                    .get::<SogStream>(root)
                    .unwrap()
                    .snapshot
                    .is_pending()
            );
        }
        let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from(Transform::from_rotation(
                Quat::from_rotation_y(std::f32::consts::PI),
            )));
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert_eq!(stream.active.len(), 1, "offscreen coarse stays selected");
        assert_eq!(stream.snapshot.is_pending(), pending);
        assert!(
            stream.entries[0].ready.is_some(),
            "retain reusable coverage"
        );
        assert!(
            children
                .iter()
                .all(|entity| app.world().get_entity(*entity).is_ok())
        );
        assert!(app.world().get_entity(old).is_ok());

        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::IDENTITY);
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert_eq!(stream.active.len(), 1);
        assert!(stream.entries.iter().all(|entry| entry.loading.is_none()));
        assert_eq!(
            stream.snapshot.is_pending(),
            pending,
            "turn must not resubmit unchanged coverage"
        );
    }
}

#[test]
fn reference_all_keeps_offscreen_selection_after_policy_switch() {
    use crate::streaming::SogLodPolicy;
    let mut stream = pixel_stream();
    let mut settings = SogLodSettings {
        lod_policy: SogLodPolicy::ScreenSpace,
        splat_budget: 100_000,
        ..default()
    };
    let view = selection::View {
        pixels: Some(screen_space::PixelView {
            clip_from_local: Mat4::orthographic_rh(20., 30., -5., 5., 0.1, 100.),
            viewport: Vec2::splat(200.),
        }),
        ..Vec3::ZERO.into()
    };
    stream.tick(view, 0., &settings, 100_000, None, |_, _, _, _| {
        panic!("cached")
    });
    assert_eq!(
        stream.active,
        [stream.manifest.leaves[0].lods.last().unwrap().1]
    );
    settings.lod_policy = SogLodPolicy::ReferenceDistance;
    stream.tick(view, 1., &settings, 100_000, None, |_, _, _, _| {
        panic!("cached")
    });
    assert_eq!(stream.active.len(), 1);
    assert_eq!(stream.active[0], stream.manifest.leaves[0].lods[0].1);
    assert_eq!(settings.visibility, SogVisibilityPolicy::All);
}

#[test]
fn wireframe_does_not_blink_while_intermediate_detail_is_in_backoff() {
    let mut stream = pixel_stream();
    let coarse = stream.manifest.leaves[0].lods[2].1;
    stream.active = vec![coarse];
    stream.submitted = vec![coarse];
    stream.displayed = vec![coarse];
    stream.entries[0].ready = None;
    stream.entries[1].ready = None;
    stream.entries[1].attempts = 1;
    stream.entries[1].retry_at = 100.;
    stream.tick(
        Vec3::ZERO.into(),
        0.,
        &SogLodSettings::default(),
        100_000,
        None,
        |_, _, _, _| panic!("retry is not due"),
    );
    assert!(
        stream.pending_work,
        "retry must still wake the streaming scheduler"
    );
    assert_eq!(
        overlay::refinement_state(&stream, 0, 5),
        super::overlay::RefinementState::Settled
    );
    let mut requests = Vec::new();
    stream.tick(
        Vec3::ZERO.into(),
        100.,
        &SogLodSettings::default(),
        100_000,
        None,
        |url, _, _, now| {
            requests.push(url);
            failed_captured_chunk(now)
        },
    );
    assert_eq!(requests, ["https://example.test/middle/meta.json"]);
    assert_eq!(
        overlay::refinement_state(&stream, 0, 5),
        super::overlay::RefinementState::Loading
    );
    assert_eq!(
        stream.entries[0].attempts, 0,
        "final target is not the intermediate download"
    );
}

#[test]
fn screen_space_stream_coarsens_and_reuses_fine_cache_on_return() {
    use crate::streaming::SogLodPolicy;
    let mut stream = pixel_stream();
    let settings = SogLodSettings {
        lod_policy: SogLodPolicy::ScreenSpace,
        splat_budget: 100_000,
        ..default()
    };
    for (step, (span, expected)) in [(2., 0), (5., 2), (20., 5), (2., 0)]
        .into_iter()
        .enumerate()
    {
        let view = selection::View {
            pixels: Some(screen_space::PixelView {
                clip_from_local: Mat4::orthographic_rh(
                    -span / 2.,
                    span / 2.,
                    -span / 2.,
                    span / 2.,
                    0.1,
                    1000.,
                ),
                viewport: Vec2::splat(200.),
            }),
            ..Vec3::ZERO.into()
        };
        stream.tick(view, step as f64, &settings, 100_000, None, |_, _, _, _| {
            panic!("reuse resident source")
        });
        let chosen = stream.manifest.leaves[0]
            .lods
            .iter()
            .find(|(_, range)| stream.active.contains(range))
            .expect("coverage")
            .0;
        assert_eq!(chosen, expected);
        assert!(
            !stream.admission.selection_limited,
            "quality target is not a budget failure"
        );
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        assert_eq!(
            overlay::refinement_state(&stream, 0, chosen),
            super::overlay::RefinementState::Settled
        );
        assert!(
            stream.entries.iter().all(|entry| entry.ready.is_some()),
            "unpressured cache retains return-path detail"
        );
    }
}

#[test]
fn screen_space_runtime_reads_main_camera_viewport_and_live_transform() {
    use crate::streaming::SogLodPolicy;
    let (mut app, root, camera) = stream_app(SogLodSettings {
        lod_policy: SogLodPolicy::ScreenSpace,
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert((pixel_stream(), GlobalTransform::IDENTITY));
    app.world_mut()
        .entity_mut(camera)
        .insert(Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.,
            ..default()
        }));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            viewport: Some(bevy::camera::Viewport {
                physical_size: UVec2::splat(8000),
                ..default()
            }),
            ..default()
        },
    ));
    for (step, (pixels, z, expected)) in [(200, 0., 2), (2000, 0., 0), (2000, 100., 2)]
        .into_iter()
        .enumerate()
    {
        app.world_mut()
            .get_mut::<Camera>(camera)
            .expect("camera")
            .viewport = Some(bevy::camera::Viewport {
            physical_size: UVec2::splat(pixels),
            ..default()
        });
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from_translation(Vec3::new(0., 0., z)));
        // Exercise the live system's projection/viewport source independently
        // of renderer-readiness (covered by the generation handoff tests).
        {
            let mut stream = app.world_mut().get_mut::<SogStream>(root).expect("stream");
            stream.snapshot = SnapshotState::default();
            stream.next_update = 0.;
        }
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert_eq!(
            app.world()
                .get::<SogStream>(root)
                .expect("stream")
                .quality_history,
            [expected],
            "step {step}"
        );
    }
}

mod scheduling;

fn shared_relocation_stream(fine_already_active: bool) -> SogStream {
    use manifest::{Bounds, Leaf};
    let range = |file, offset, count| SplatRange {
        file,
        offset,
        count,
    };
    let leaf = |x, lods| Leaf {
        bound: Bounds {
            min: Vec3::new(x, -1., -1.),
            max: Vec3::new(x + 1., 1., 1.),
        },
        lods,
    };
    let mut stream = relocation_stream();
    if fine_already_active {
        stream.manifest.file_counts = vec![4, 12, 8];
        stream.manifest.leaves = vec![
            leaf(
                0.,
                vec![
                    (0, range(2, 0, 8)),
                    (1, range(1, 0, 6)),
                    (2, range(0, 0, 2)),
                ],
            ),
            leaf(20., vec![(1, range(1, 6, 6)), (2, range(0, 2, 2))]),
        ];
        stream.active = vec![range(2, 0, 8), range(0, 2, 2)];
    } else {
        stream.manifest.file_counts = vec![4, 8, 4, 12, 8];
        stream.manifest.leaves = vec![
            leaf(4., vec![(0, range(1, 0, 8)), (3, range(0, 0, 1))]),
            leaf(
                0.,
                vec![
                    (0, range(4, 0, 8)),
                    (1, range(3, 0, 6)),
                    (2, range(2, 0, 4)),
                    (3, range(0, 1, 1)),
                ],
            ),
            leaf(20., vec![(1, range(3, 6, 6)), (3, range(0, 2, 1))]),
            leaf(30., vec![(3, range(0, 3, 1))]),
        ];
        stream.active = vec![
            range(1, 0, 8),
            range(2, 0, 4),
            range(0, 2, 1),
            range(0, 3, 1),
        ];
    }
    stream.manifest.filenames = (0..stream.manifest.file_counts.len())
        .map(|i| format!("{i}/meta.json"))
        .collect();
    stream.entries = stream
        .manifest
        .file_counts
        .iter()
        .enumerate()
        .map(|(i, count)| Entry {
            ready: (fine_already_active || i != 4).then(|| chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); 32].into(),
                count: *count,
            }),
            ..default()
        })
        .collect();
    stream.submitted = stream.active.clone();
    stream.displayed = stream.active.clone();
    stream
}

fn blocked_intermediate_stream() -> SogStream {
    let mut stream = shared_relocation_stream(true);
    // The intermediate packs both leaves into one large file. The nearby
    // leaf's independent LOD0 file fits, even though its predecessor doesn't.
    stream.manifest.file_counts[1] = 40;
    stream.manifest.leaves[1].lods[0].1.count = 34;
    stream.entries[1].ready = None;
    stream.entries[2].ready = None;
    stream.active = stream
        .manifest
        .leaves
        .iter()
        .map(|leaf| leaf.lods.last().expect("coarse").1)
        .collect();
    stream.submitted = stream.active.clone();
    stream.displayed = stream.active.clone();
    stream
}

#[test]
fn relocation_skips_an_unaffordable_intermediate_to_reach_independent_fine_detail() {
    for gpu_pressure in [false, true] {
        let mut stream = blocked_intermediate_stream();
        stream.cpu_timings = SogCpuTimings::enabled();
        let settings = SogLodSettings {
            splat_budget: 20,
            cache_bytes: (if gpu_pressure { 128 } else { 64 })
                * chunk::DecodedChunk::bytes_per_splat(),
            ..default()
        };
        let mut requests = Vec::new();
        for time in 0..5 {
            stream.tick(
                Vec3::ZERO.into(),
                f64::from(time),
                &settings,
                1000,
                gpu_pressure.then_some(12 * renderer::source_bytes_per_splat()),
                |url, _, _, now| {
                    requests.push(url);
                    failed_captured_chunk(now)
                },
            );
            // Simulate completed transport/decode; selection and admission are real.
            if stream.entries[2].loading.is_some() {
                stream.entries[2].complete(
                    Ok(chunk::DecodedChunk {
                        cloud: vec![Gaussian3d::default(); 32].into(),
                        count: 8,
                    }),
                    f64::from(time),
                );
            }
            stream.submitted = stream.active.clone();
            stream.displayed = stream.active.clone();
            assert_eq!(stream.active.len(), 2, "retain both leaves' coverage");
            assert!(stream.entries[1].ready.is_none());
            let status = stream.status();
            assert!(
                status.cache_ready_bytes + status.cache_reserved_bytes
                    <= settings.cache_bytes as u64
            );
        }
        assert_eq!(
            requests,
            ["https://example.test/2/meta.json"],
            "request independent LOD0 once, without fetching the oversized intermediate (GPU pressure: {gpu_pressure})"
        );
        assert_eq!(
            stream
                .active
                .iter()
                .map(|range| range.file)
                .collect::<Vec<_>>(),
            [2, 0]
        );
        assert!(
            !stream.pending_work,
            "settle after reaching available fine detail"
        );
        let cpu = stream.cpu_timings.report().cpu_stages.expect("enabled");
        assert!(cpu.relocation_plan.attempts > 0);
        assert!(cpu.relocation_plan.total_ms.is_some());
        assert_eq!(
            cpu.decode.attempts, 0,
            "fixture supplies decoded clouds directly"
        );
    }
}

#[test]
fn direct_relocation_retries_with_backoff_then_settles_on_coverage() {
    let mut stream = blocked_intermediate_stream();
    let settings = SogLodSettings {
        splat_budget: 20,
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    };
    let mut request_times = Vec::new();
    for now in [0., 1., 2., 3., 6., 7., 14., 100.] {
        stream.tick(
            Vec3::ZERO.into(),
            now,
            &settings,
            1000,
            None,
            |url, _, _, now| {
                assert_eq!(url, "https://example.test/2/meta.json");
                request_times.push(now);
                failed_captured_chunk(now)
            },
        );
        if stream.entries[2].loading.is_some() {
            stream.entries[2].complete(Err("transport failed".into()), now);
        }
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        assert_eq!(
            stream
                .active
                .iter()
                .map(|range| range.file)
                .collect::<Vec<_>>(),
            [0, 0]
        );
    }
    assert_eq!(
        request_times,
        [0., 2., 6.],
        "no per-tick or unbounded retries"
    );
    assert!(
        !stream.pending_work,
        "exhausted destination must let redraw sleep"
    );
}

#[test]
fn relocation_keeps_valid_fine_detail_when_shared_intermediate_is_impossible() {
    let mut stream = shared_relocation_stream(true);
    let before = stream.active.clone();
    for time in 0..5 {
        stream.tick(
            Vec3::ZERO.into(),
            f64::from(time),
            &SogLodSettings {
                splat_budget: 20,
                ..default()
            },
            1000,
            Some(12 * renderer::source_bytes_per_splat()),
            |_, _, _, _| panic!("all useful files are resident"),
        );
        assert_eq!(
            stream.active, before,
            "do not lose detail merely because its intermediate is too large"
        );
        stream.submitted = stream.active.clone();
    }
}

#[test]
fn relocation_retries_its_destination_after_evicting_the_cached_intermediate() {
    let mut stream = shared_relocation_stream(true);
    stream.entries[2].ready = None;
    stream.active = stream
        .manifest
        .leaves
        .iter()
        .map(|leaf| leaf.lods.last().expect("coarse").1)
        .collect();
    stream.submitted = stream.active.clone();
    let settings = SogLodSettings {
        splat_budget: 10,
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    };
    let mut requests = Vec::new();
    for time in [0., 100.] {
        stream.tick(
            Vec3::ZERO.into(),
            time,
            &settings,
            1000,
            Some(12 * renderer::source_bytes_per_splat()),
            |url, _, _, now| {
                requests.push(url);
                failed_captured_chunk(now)
            },
        );
        assert!(
            stream.entries[1].ready.is_none(),
            "intermediate must be evicted for destination"
        );
        stream.entries[2].complete(Err("transient network failure".into()), time);
        stream.submitted = stream.active.clone();
    }
    assert_eq!(
        requests,
        [
            "https://example.test/2/meta.json",
            "https://example.test/2/meta.json"
        ],
        "a retryable established destination must not depend on its evicted intermediate"
    );
}

#[test]
fn relocation_reaches_destination_without_restoring_an_obsolete_intermediate() {
    let mut stream = shared_relocation_stream(false);
    let mut requested = Vec::new();
    for time in 0..10 {
        stream.tick(
            Vec3::ZERO.into(),
            f64::from(time),
            &SogLodSettings {
                splat_budget: 18,
                ..default()
            },
            1000,
            Some(20 * renderer::source_bytes_per_splat()),
            |url, _, _, now| {
                requested.push(url);
                failed_captured_chunk(now)
            },
        );
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        if stream.entries[4].loading.is_some() {
            stream.entries[4].loading = None;
            stream.entries[4].reserved_splats = None;
            stream.entries[4].ready = Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); 32].into(),
                count: 8,
            });
        }
    }
    assert_eq!(
        requested,
        ["https://example.test/4/meta.json"],
        "replacement must be downloaded exactly once"
    );
    assert_eq!(
        stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
        [1, 4, 0, 0]
    );
}

/// Two separated leaves: coarse coverage costs 4 rows; each detail file costs 8.
/// A 12-row GPU cache (or two padded CPU files) fits either detailed region,
/// but cannot hold both. This catches a residency-dependent camera lock.
fn relocation_stream() -> SogStream {
    let mut stream = SogStream::parse(
        br#"{"lodLevels":2,"filenames":["coarse/meta.json","left/meta.json","right/meta.json"],
        "tree":{"bound":{"min":[-10,-1,-1],"max":[10,1,1]},"children":[
            {"bound":{"min":[-10,-1,-1],"max":[-8,1,1]},"lods":{
                "0":{"file":1,"offset":0,"count":8},"1":{"file":0,"offset":0,"count":2}}},
            {"bound":{"min":[8,-1,-1],"max":[10,1,1]},"lods":{
                "0":{"file":2,"offset":0,"count":8},"1":{"file":0,"offset":2,"count":2}}}
        ]}}"#,
        "https://example.test/lod-meta.json",
        "relocation".into(),
        "relocation".into(),
    )
    .expect("valid fixture");
    for (file, count) in [(0, 4), (1, 8)] {
        stream.entries[file].ready = Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); 32].into(),
            count,
        });
    }
    stream
}

#[test]
fn leaf_evidence_distinguishes_decoded_selected_submitted_and_displayed_detail() {
    let mut stream = relocation_stream();
    stream.configure_diagnostics(true);
    stream.submitted = stream
        .manifest
        .leaves
        .iter()
        .map(|leaf| leaf.lods[1].1)
        .collect();
    stream.displayed = stream.submitted.clone();
    stream.tick(
        Vec3::new(-9., 0., 0.).into(),
        0.,
        &SogLodSettings {
            splat_budget: 10,
            ..default()
        },
        1000,
        None,
        |_, _, _, _| panic!("left detail already decoded"),
    );
    let before = &stream.leaf_snapshot(2)[0];
    assert_eq!(before.finest_available_lod, Some(0));
    assert_eq!(before.resident_lod, Some(0));
    assert_eq!(before.selected_lod, Some(0));
    assert_eq!(before.submitted_lod, Some(1));
    assert_eq!(before.displayed_lod, Some(1));
    assert_eq!(before.target_source_state, "resident");
    assert_eq!(before.target_file.as_deref(), Some("left/meta.json"));
    // The exported evidence must retain exact stage ranges, not just LOD labels.
    let serialized = serde_json::to_value(before).expect("serializable leaf evidence");
    assert_eq!(
        serialized["selected_range"],
        serde_json::json!({"file":1,"offset":0,"count":8})
    );
    assert_eq!(
        serialized["submitted_range"],
        serde_json::json!({"file":0,"offset":0,"count":2})
    );
    assert_eq!(
        serialized["displayed_range"],
        serde_json::json!({"file":0,"offset":0,"count":2})
    );
    stream.submitted = stream.active.clone();
    assert_eq!(stream.leaf_snapshot(1)[0].submitted_lod, Some(0));
    assert_eq!(stream.leaf_snapshot(1)[0].displayed_lod, Some(1));
    // Readiness can change between policy ticks; publishing must not use a stale trace.
    stream.displayed = stream.submitted.clone();
    assert_eq!(stream.leaf_snapshot(1)[0].displayed_lod, Some(0));
    assert_eq!(
        serde_json::to_value(&stream.leaf_snapshot(1)[0]).expect("serializable leaf evidence")["displayed_range"],
        serde_json::json!({"file":1,"offset":0,"count":8})
    );
    assert!(stream.leaf_snapshot(0).is_empty());
    stream.configure_diagnostics(false);
    assert!(stream.leaf_snapshot(256).is_empty());
}

#[test]
fn leaf_evidence_does_not_call_failed_or_unrequested_targets_loading() {
    let mut stream = relocation_stream();
    stream.configure_diagnostics(true);
    stream.tick(
        Vec3::new(-9., 0., 0.).into(),
        0.,
        &SogLodSettings {
            splat_budget: 10,
            ..default()
        },
        1000,
        None,
        |_, _, _, _| panic!("resident target"),
    );
    stream.entries[1].ready = None;
    assert_eq!(
        stream.leaf_snapshot(1)[0].target_source_state,
        "not requested"
    );
    stream.entries[1].attempts = 1;
    assert_eq!(
        stream.leaf_snapshot(1)[0].target_source_state,
        "retry pending"
    );
    stream.entries[1].attempts = 3;
    assert_eq!(stream.leaf_snapshot(1)[0].target_source_state, "failed");
    stream.entries[1].loading = Some(failed_captured_chunk(0.));
    assert_eq!(
        stream.leaf_snapshot(1)[0].target_source_state,
        "loading/decode"
    );
    assert_eq!(stream.leaf_snapshot(1)[0].resident_lod, Some(1));
}

#[test]
fn wireframe_refinement_tracks_queue_handoff_failure_and_camera_cancellation() {
    use super::overlay::RefinementState::{Capped, Loading, Settled};
    let mut stream = relocation_stream();
    stream.active.clear();
    stream.submitted.clear();
    let fine = stream.manifest.leaves[0].lods[0].1;
    let coarse = stream.manifest.leaves[0].lods.last().unwrap().1;
    let coarse_level = stream.manifest.leaves[0].lods.last().unwrap().0;
    stream.displayed = vec![coarse];
    stream.overlay_intent.targets = vec![Some(fine), None];
    stream.overlay_intent.queued_targets = vec![Some(fine), None];
    stream.entries[fine.file].ready = None;
    stream.pending_work = true;
    assert_eq!(
        overlay::refinement_state(&stream, 0, coarse_level),
        Loading,
        "queued admitted target"
    );
    stream.overlay_intent.blocked_files = vec![true; stream.entries.len()];
    assert_eq!(overlay::refinement_state(&stream, 0, coarse_level), Capped);
    stream.entries[fine.file].attempts = 1;
    stream.entries[fine.file].loading = Some(failed_captured_chunk(0.));
    assert_eq!(
        overlay::refinement_state(&stream, 0, coarse_level),
        Loading,
        "actual in-flight work wins over a cap"
    );
    stream.entries[fine.file].complete(Err("test failure".into()), 1.);
    stream.overlay_intent.queued_targets.clear();
    assert_eq!(
        overlay::refinement_state(&stream, 0, coarse_level),
        Settled,
        "backoff is not download progress"
    );
    stream.entries[fine.file].attempts = 3;
    assert_eq!(overlay::refinement_state(&stream, 0, coarse_level), Settled);
    stream.submitted = vec![fine];
    assert_eq!(
        overlay::refinement_state(&stream, 0, coarse_level),
        Loading,
        "accepted generation not displayed yet"
    );
    stream.submission_error = Some("renderer rejected".into());
    assert_eq!(overlay::refinement_state(&stream, 0, coarse_level), Settled);
    stream.submission_error = None;
    stream.overlay_intent.targets[0] = None;
    assert_eq!(
        overlay::refinement_state(&stream, 0, coarse_level),
        Settled,
        "cancelled/offscreen intent cannot blink"
    );
    stream.overlay_intent.targets[0] = Some(fine);
    assert_eq!(
        overlay::refinement_state(&stream, 0, 0),
        Settled,
        "displayed finest detail stops blinking immediately"
    );
    assert!(stream.diagnostics.is_none());
}

#[test]
fn relocation_releases_old_detail_before_requesting_new_region_under_cpu_or_gpu_pressure() {
    for gpu_pressure in [false, true] {
        let mut stream = relocation_stream();
        let settings = SogLodSettings {
            splat_budget: 10,
            cache_bytes: (if gpu_pressure { 256 } else { 64 })
                * chunk::DecodedChunk::bytes_per_splat(),
            ..default()
        };
        let gpu_bytes = gpu_pressure.then_some(12 * renderer::source_bytes_per_splat());
        stream.tick(
            Vec3::new(-9., 0., 0.).into(),
            0.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("left detail is already resident"),
        );
        assert_eq!(
            stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
            [1, 0]
        );
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();

        stream.tick(
            Vec3::new(9., 0., 0.).into(),
            1.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("old generation must retire before downloading new detail"),
        );
        assert_eq!(
            stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0],
            "must publish a covered release generation, GPU pressure={gpu_pressure}"
        );
        assert!(
            stream.entries[1].ready.is_some(),
            "old source still pinned until readiness"
        );
        assert_eq!(
            stream.displayed[0].file, 1,
            "unready bridge must not change displayed detail"
        );

        // Renderer readiness acknowledges the covered bridge before the next tick.
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        let mut requests = Vec::new();
        stream.tick(
            Vec3::new(9., 0., 0.).into(),
            2.,
            &settings,
            1000,
            gpu_bytes,
            |url, _, _, now| {
                requests.push(url);
                failed_captured_chunk(now)
            },
        );
        assert_eq!(requests, ["https://example.test/right/meta.json"]);
        if !gpu_pressure {
            assert!(
                stream.entries[1].ready.is_none(),
                "obsolete CPU source must be evicted"
            );
        }
        assert_eq!(
            stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );
        stream.entries[2].loading = None;
        stream.entries[2].reserved_splats = None;
        stream.entries[2].ready = Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); 32].into(),
            count: 8,
        });
        for time in [3., 4., 5.] {
            stream.tick(
                Vec3::new(9., 0., 0.).into(),
                time,
                &settings,
                1000,
                gpu_bytes,
                |_, _, _, _| panic!("new region must settle without redownloading"),
            );
            assert_eq!(
                stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
                [0, 2]
            );
            stream.submitted = stream.active.clone();
        }
    }
}

#[test]
fn relocation_waits_for_covered_generation_readiness_before_releasing_sources() {
    let settings = SogLodSettings {
        splat_budget: 10,
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    };
    let (mut app, root, camera) = stream_app(settings);
    app.world_mut()
        .entity_mut(root)
        .insert((relocation_stream(), GlobalTransform::IDENTITY));
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(-9., 0., 0.)));
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(9., 0., 0.)));
    advance_stream(&mut app);
    let bridge = app
        .world()
        .get::<Children>(root)
        .expect("snapshots")
        .iter()
        .find(|entity| *entity != old)
        .expect("covered bridge");
    for _ in 0..3 {
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).expect("stream");
        assert_eq!(
            stream.displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
            [1, 0]
        );
        assert_eq!(
            stream.submitted.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );
        assert!(stream.entries[1].ready.is_some());
        assert!(
            stream.entries[2].loading.is_none(),
            "no network request before the old pin retires"
        );
        assert!(app.world().get_entity(old).is_ok());
        assert_eq!(
            app.world().get::<Visibility>(bridge),
            Some(&Visibility::Hidden)
        );
    }
}

#[test]
fn relocation_does_not_drop_detail_for_an_impossible_or_failed_destination() {
    for exhausted in [false, true] {
        let mut stream = relocation_stream();
        // Right detail is either irrecoverably failed or intrinsically too
        // large, even after left detail retires. A bridge would only flicker.
        if exhausted {
            stream.entries[2].attempts = 3;
        } else {
            stream.manifest.file_counts[2] = 80;
            stream.manifest.leaves[1].lods[0].1.count = 80;
        }
        let settings = SogLodSettings {
            splat_budget: 100,
            ..default()
        };
        let gpu_bytes = Some(12 * renderer::source_bytes_per_splat());
        stream.tick(
            Vec3::new(-9., 0., 0.).into(),
            0.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("impossible destination must not download"),
        );
        stream.submitted = stream.active.clone();
        for time in [1., 2., 3.] {
            assert!(!stream.tick(
                Vec3::new(9., 0., 0.).into(),
                time,
                &settings,
                1000,
                gpu_bytes,
                |_, _, _, _| panic!("impossible destination must not download")
            ));
            assert_eq!(
                stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
                [1, 0]
            );
            assert!(
                !stream.pending_work,
                "do not keep redraw awake for an impossible destination"
            );
        }
    }
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR; decodes every chunk and environment without a renderer"]
fn complete_local_fixture_decodes_all_chunks_with_finite_attributes() {
    use bevy_gaussian_splatting::io::sog::{decode_sog, parse_meta};

    let root =
        std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").expect("fixture directory"));
    let manifest = manifest::Manifest::parse(
        &std::fs::read(root.join("lod-meta.json")).expect("manifest bytes"),
    )
    .expect("production hierarchy validation");
    let mut total = 0usize;
    let mut chunks = 0usize;
    // Do not let camera selection, budget admission or dominated-LOD pruning
    // hide a missing/broken file. Include the independently rendered environment.
    for (index, path) in manifest
        .filenames
        .iter()
        .chain(manifest.environment.iter())
        .enumerate()
    {
        let meta_path = root.join(path);
        let bytes = std::fs::read(&meta_path).expect(path);
        let meta = parse_meta(&bytes).expect(path);
        if let Some(expected) = manifest.file_counts.get(index) {
            assert_eq!(meta.count, *expected, "hierarchy count for {path}");
        }
        let files = meta
            .referenced_files()
            .into_iter()
            .map(|name| {
                let payload = meta_path.parent().expect("chunk directory").join(name);
                (name.to_string(), std::fs::read(&payload).expect(name))
            })
            .collect();
        let cloud = decode_sog(&meta, &files).expect(path);
        assert!(cloud.len() >= meta.count && cloud.len() <= meta.count + 32);
        assert_eq!(cloud.rotation.len(), cloud.len(), "{path}");
        assert_eq!(cloud.scale_opacity.len(), cloud.len(), "{path}");
        assert_eq!(cloud.spherical_harmonic.len(), cloud.len(), "{path}");
        for i in 0..meta.count {
            let position = &cloud.position_visibility[i];
            let scale = &cloud.scale_opacity[i];
            assert!(
                position.position.iter().all(|v| v.is_finite()),
                "{path}: position {i}"
            );
            assert!(position.visibility.is_finite(), "{path}: visibility {i}");
            assert!(
                cloud.rotation[i].rotation.iter().all(|v| v.is_finite()),
                "{path}: rotation {i}"
            );
            assert!(
                scale.scale.iter().all(|v| v.is_finite() && *v > 0.),
                "{path}: scale {i}"
            );
            assert!((0.0..=1.0).contains(&scale.opacity), "{path}: opacity {i}");
            assert!(
                cloud.spherical_harmonic[i]
                    .coefficients
                    .iter()
                    .all(|v| v.is_finite()),
                "{path}: SH {i}"
            );
        }
        total += meta.count;
        chunks += 1;
        eprintln!("validated {path}: {} logical splats", meta.count);
        // Each cloud and its compressed inputs drop before decoding the next.
    }
    assert!(chunks > 0 && total > 0);
    eprintln!(
        "validated complete fixture: {chunks} chunks, {total} logical splats across all LODs"
    );
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR with the complete saved kitchen SOG dataset"]
fn captured_camera_relocation_settles_and_refines_new_regions_without_growing_cache() {
    let root =
        std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").expect("fixture directory"));
    let mut stream = SogStream::parse(
        &std::fs::read(root.join("lod-meta.json")).expect("manifest"),
        "https://capture/lod-meta.json",
        "relocation".into(),
        "relocation".into(),
    )
    .expect("saved manifest");
    let settings = SogLodSettings {
        splat_budget: 1_000_000,
        cache_bytes: 256 * 1024 * 1024,
        ..default()
    };
    let cameras = [
        (Vec3::new(-2.01, 0.127, -6.347), 90_f32),
        (Vec3::new(8., 1., -35.), 90_f32),
        (Vec3::new(8., 1., -35.), 45_f32),
        (Vec3::new(-2.01, 0.127, -6.347), 90_f32),
    ];
    stream.configure_diagnostics(true);
    let mut selections = Vec::new();
    let mut requests = Vec::new();
    let mut observations = Vec::new();
    for (phase, (eye, fov_degrees)) in cameras.into_iter().enumerate() {
        let projection =
            Mat4::perspective_infinite_reverse_rh(fov_degrees.to_radians(), 16. / 9., 0.01);
        let clip_from_local = projection * Mat4::look_to_rh(eye, Vec3::NEG_Z, Vec3::Y);
        let view = selection::View {
            eye,
            forward: Vec3::NEG_Z,
            fov_scale: (fov_degrees.to_radians() * 0.5).tan() / std::f32::consts::FRAC_PI_8.tan(),
            clip_from_local: Some(clip_from_local),
            ..Vec3::ZERO.into()
        };
        let mut settled = false;
        let request_start = requests.len();
        let mut ticks = 0;
        for tick in 0..200 {
            ticks = tick + 1;
            let now = (phase * 200 + tick) as f64;
            stream.tick(
                view,
                now,
                &settings,
                1_000_000,
                None,
                |url, expected, _, now| {
                    let path = url.strip_prefix("https://capture/").expect("fixture URL");
                    requests.push(path.to_string());
                    let meta_path = root.join(path);
                    let bytes = std::fs::read(&meta_path).expect("metadata");
                    let meta = bevy_gaussian_splatting::io::sog::parse_meta(&bytes)
                        .expect("metadata schema");
                    if let Some(count) = expected {
                        assert_eq!(meta.count, count);
                    }
                    let files = meta
                        .referenced_files()
                        .iter()
                        .map(|name| {
                            (
                                name.to_string(),
                                std::fs::read(
                                    meta_path.parent().expect("chunk directory").join(name),
                                )
                                .expect("payload"),
                            )
                        })
                        .collect();
                    ChunkLoad::captured(&bytes, files, now)
                },
            );
            if stream.active != stream.submitted {
                // Exercise real snapshot composition before acknowledging this
                // generation. The ECS regression separately holds readiness off.
                let cloud = compose::compose(&stream.active, |i| stream.entries[i].ready.as_ref())
                    .expect("covered generation");
                assert!(cloud.len() <= settings.splat_budget + 32);
                stream.submitted = stream.active.clone();
                stream.displayed = stream.active.clone();
            }
            let status = stream.status();
            assert!(
                status.cache_ready_bytes + status.cache_reserved_bytes
                    <= settings.cache_bytes as u64
            );
            if !stream.pending_work {
                settled = true;
                break;
            }
        }
        eprintln!(
            "relocation phase {phase}: {} requests, selected {}, settled {settled}",
            requests.len() - request_start,
            stream.status().selected
        );
        assert!(
            settled,
            "relocation must converge instead of cycling release/refinement"
        );
        assert!(stream.last_error.is_none(), "{:?}", stream.last_error);
        if phase != 2 {
            assert!(
                requests.len() > request_start,
                "moving regions must acquire replacement detail, not only reshuffle cached ranges"
            );
        }
        // Offline counterfactual only: can a directly decodable final file set
        // improve this settled state without increasing the same source cap?
        let costs: Vec<_> = stream
            .entries
            .iter()
            .enumerate()
            .map(|(file, entry)| {
                entry.ready.as_ref().map_or_else(
                    || {
                        cache::reserved_rows(
                            stream.manifest.file_counts.get(file).copied().unwrap_or(0),
                        )
                    },
                    |chunk| chunk.padded_rows(),
                )
            })
            .collect();
        let mut fixed = vec![false; costs.len()];
        for file in stream
            .manifest
            .coarse_files()
            .into_iter()
            .chain(stream.environment_index())
        {
            fixed[file] = true;
        }
        let mut preferred = fixed.clone();
        for range in &stream.active {
            preferred[range.file] = true;
        }
        let admissions = [selection::SourceAdmission {
            costs: &costs,
            pinned: &fixed,
            capacity: settings.cache_bytes / chunk::DecodedChunk::bytes_per_splat(),
        }];
        let environment_count = stream.environment_range().map_or(0, |r| r.count);
        let mut policy_view = view;
        policy_view.clip_from_local = None; // Preserve this replay's All visibility policy.
        let direct = relocation::plan_files(
            &stream.manifest,
            policy_view,
            settings.splat_budget.saturating_sub(environment_count),
            &admissions,
            &fixed,
            &preferred,
            |_| true,
        );
        let direct_ranges = relocation::select_files(
            &stream.manifest,
            policy_view,
            settings.splat_budget.saturating_sub(environment_count),
            &admissions,
            &direct,
        );
        let reachable = next_files(&stream.manifest, &stream.entries);
        let direct_files: Vec<_> = stream
            .manifest
            .filenames
            .iter()
            .enumerate()
            .filter(|(i, _)| direct[*i])
            .map(|(i, path)| {
                serde_json::json!({
                    "file": path, "reachable_by_current_prefetch": reachable[i],
                    "cpu_bytes": costs[i] * chunk::DecodedChunk::bytes_per_splat(),
                })
            })
            .collect();
        observations.push(serde_json::json!({
            "phase": phase, "eye_local": eye.to_array(), "forward_local": [0, 0, -1],
            "vertical_fov_degrees": fov_degrees, "aspect": 16. / 9.,
            "clip_from_local": clip_from_local.to_cols_array(),
            "settled": settled, "ticks": ticks,
            "requests": &requests[request_start..],
            "selected": stream.status().selected,
            "cache_ready_bytes": stream.status().cache_ready_bytes,
            "leaves_total": stream.manifest.leaves.len(),
            "leaves": stream.leaf_snapshot(diagnostics::MAX_LEAF_DIAGNOSTICS),
            "counterfactual_direct_files": direct_files,
            "counterfactual_direct_cpu_bytes": costs.iter().enumerate().filter(|(i, _)| direct[*i]).map(|(_, cost)| *cost * chunk::DecodedChunk::bytes_per_splat()).sum::<usize>(),
            "counterfactual_direct_selected": direct_ranges.iter().flatten().map(|r| r.count).sum::<usize>() + environment_count,
            "counterfactual_direct_changes": direct != preferred,
        }));
        selections.push(stream.active.clone());
    }
    assert_ne!(
        selections[0], selections[1],
        "detail must follow the moved eye"
    );
    assert_ne!(
        selections[1], selections[3],
        "detail must also follow the return journey"
    );
    if let Ok(path) = std::env::var("SOG_TRACE_OUTPUT") {
        let evidence = serde_json::json!({
            "scope": "filesystem_policy_replay_no_gpu",
            "handoff": "acknowledged after CPU composition; not actual GPU readiness",
            "fixture": root,
            "compiled_sh_degree": bevy_gaussian_splatting::material::spherical_harmonics::SH_DEGREE,
            "visibility": "all", "requested_budget": settings.splat_budget,
            "cache_bytes": settings.cache_bytes, "gpu_selection_limit": 1_000_000,
            "gpu_source_limit": null, "phases": observations,
        });
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&evidence).expect("trace JSON"),
        )
        .expect("write diagnostic evidence");
    }
}

#[test]
fn failed_coarse_coverage_stops_without_fanning_out_to_larger_levels() {
    let mut stream = SogStream::parse(
        SCENE,
        "https://capture/lod-meta.json",
        "capture".into(),
        "capture".into(),
    )
    .unwrap();
    for i in stream
        .manifest
        .coarse_files()
        .into_iter()
        .chain(stream.environment_index())
    {
        stream.entries[i].attempts = 3;
    }
    stream.last_error = Some("network unavailable".into());
    assert!(!stream.tick(
        Vec3::ZERO.into(),
        100.0,
        &SogLodSettings::default(),
        gpu_splat_limit(128 * 1024 * 1024),
        None,
        |_, _, _, _| panic!("must not start another request")
    ));
    assert!(!stream.pending_work);
    assert!(stream.last_error.is_some());
}

/// Build a decoded neutral two-level stream for lifecycle tests.
pub(super) fn tiny_stream() -> SogStream {
    let mut stream = SogStream::parse(
        br#"{
        "lodLevels":1,"filenames":["coarse/meta.json"],
        "tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},
        "lods":{"0":{"file":0,"offset":0,"count":1}}}
    }"#,
        "https://example.test/lod-meta.json",
        "test".into(),
        "sog-test".into(),
    )
    .unwrap();
    stream.entries[0].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default()].into(),
        count: 1,
    });
    stream
}

#[test]
fn quality_effective_bound_includes_preserved_coarse_coverage_floor() {
    let mut stream = tiny_stream();
    let settings = SogLodSettings {
        splat_budget: 0,
        ..default()
    };
    stream.tick(Vec3::ZERO.into(), 0.0, &settings, 32, None, |_, _, _, _| {
        panic!("coarse source already resident")
    });
    assert_eq!(
        stream.active.iter().map(|range| range.count).sum::<usize>(),
        1
    );
    assert_eq!(stream.status().requested, 0);
    assert_eq!(stream.status().effective, 1);
    assert!(
        stream
            .status()
            .limiting_reason
            .contains("coarse coverage floor")
    );
    let settings = SogLodSettings {
        splat_budget: 64,
        ..default()
    };
    stream.tick(Vec3::ZERO.into(), 1.0, &settings, 32, None, |_, _, _, _| {
        panic!("coarse source already resident")
    });
    assert_eq!(stream.status().requested, 64);
    assert_eq!(stream.status().effective, 32);
    assert!(
        !stream
            .status()
            .limiting_reason
            .contains("coarse coverage floor")
    );
}

#[test]
fn retains_pinned_fine_after_other_scene_reduces_gpu_headroom() {
    let mut stream = tiny_stream();
    stream
        .manifest
        .filenames
        .extend(["middle/meta.json".into(), "fine/meta.json".into()]);
    stream.manifest.file_counts = vec![4, 8, 8];
    stream.manifest.leaves[0].lods = vec![
        (
            0,
            SplatRange {
                file: 2,
                offset: 0,
                count: 8,
            },
        ),
        (
            1,
            SplatRange {
                file: 1,
                offset: 0,
                count: 6,
            },
        ),
        (
            2,
            SplatRange {
                file: 0,
                offset: 0,
                count: 4,
            },
        ),
    ];
    stream.entries = [4, 8, 8]
        .into_iter()
        .map(|count| Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); count].into(),
                count,
            }),
            ..default()
        })
        .collect();
    stream.diagnostics = Some(Vec::new());
    let bytes = renderer::source_bytes_per_splat();
    let settings = SogLodSettings::default();
    assert!(stream.tick(
        Vec3::ZERO.into(),
        0.,
        &settings,
        1000,
        Some(20 * bytes),
        |_, _, _, _| panic!("cached sources must not download")
    ));
    assert_eq!(
        stream.active,
        [SplatRange {
            file: 2,
            offset: 0,
            count: 8
        }]
    );
    stream.submitted = stream.active.clone();
    assert!(!stream.tick(
        Vec3::ZERO.into(),
        1.,
        &settings,
        1000,
        Some(12 * bytes),
        |_, _, _, _| panic!("cached sources must not download")
    ));
    assert_eq!(stream.active, stream.submitted);
    assert_eq!(stream.diagnostics.as_ref().unwrap()[0].chosen_lod, Some(0));
    assert!(!stream.pending_work);
}

#[test]
fn impossible_source_limit_retains_selection_and_settles_until_limit_changes() {
    let mut stream = tiny_stream();
    let settings = SogLodSettings::default();
    stream.tick(
        Vec3::ZERO.into(),
        0.,
        &settings,
        1000,
        None,
        |_, _, _, _| panic!("resident"),
    );
    let old = stream.active.clone();
    let impossible = SogLodSettings {
        cache_bytes: 0,
        ..settings.clone()
    };
    for time in [1., 2., 3.] {
        assert!(!stream.tick(
            Vec3::ZERO.into(),
            time,
            &impossible,
            1000,
            None,
            |_, _, _, _| panic!("impossible requests must not retry")
        ));
        assert_eq!(stream.active, old);
        assert!(!stream.pending_work);
        assert!(stream.last_error.is_some());
    }
    stream.tick(
        Vec3::ZERO.into(),
        4.,
        &settings,
        1000,
        None,
        |_, _, _, _| panic!("resident"),
    );
    assert!(stream.last_error.is_none());
}

#[test]
fn all_visibility_diagnostics_project_without_enabling_culling() {
    let mut stream = tiny_stream();
    stream.diagnostics = Some(Vec::new());
    stream.tick(
        selection::View {
            clip_from_local: Some(Mat4::orthographic_rh(8., 10., -2., 2., -2., 2.)),
            ..Vec3::ZERO.into()
        },
        0.,
        &SogLodSettings {
            lod_policy: SogLodPolicy::ReferenceDistance,
            ..default()
        },
        1000,
        None,
        |_, _, _, _| panic!("resident"),
    );
    assert_eq!(stream.active.len(), 1);
    let trace = stream.diagnostics.as_ref().unwrap();
    assert_eq!(trace.len(), 1);
    assert!(!trace[0].culled);
    assert!(trace[0].projected_bounds.is_some());
    assert_eq!(trace[0].range, Some(stream.active[0]));
}

#[test]
fn refinement_exposes_only_one_level_beyond_resident_coverage() {
    let mut stream = SogStream::parse(
        SCENE,
        "https://example.test/lod-meta.json",
        "test".into(),
        "test".into(),
    )
    .unwrap();
    stream.entries[5].ready = Some(chunk::DecodedChunk {
        cloud: default(),
        count: 404090,
    });
    let next = next_files(&stream.manifest, &stream.entries);
    for leaf in &stream.manifest.leaves {
        for (level, range) in &leaf.lods {
            if *level < 4 {
                assert!(!next[range.file]);
            }
        }
    }
    assert!(next[4]);
}

#[test]
fn replacement_waits_for_gpu_readiness_and_despawning_cleans_progress() {
    let mut app = App::new();
    renderer::readiness::install(&mut app);
    // Exercise stream policy/readiness with CPU fixtures, independently of
    // the production packed renderer (covered by packed_handoff_tests).
    app.insert_resource(renderer::SogRendererBackend::Snapshot);
    app.init_resource::<Assets<bevy_gaussian_splatting::sort::SortedEntries>>();
    app.init_resource::<Time<Real>>()
        .insert_resource(SogLodSettings {
            lod_policy: SogLodPolicy::ReferenceDistance,
            ..default()
        })
        .init_resource::<Assets<PlanarGaussian3d>>()
        .init_resource::<ReadySnapshots>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
        .add_plugins((
            bevy::transform::TransformPlugin,
            bevy::camera::visibility::VisibilityPlugin,
        ))
        .add_systems(
            PostUpdate,
            update_sog_streams
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
        );
    app.world_mut().spawn((
        StreamingCamera,
        Camera3d::default(),
        Projection::default(),
        GlobalTransform::IDENTITY,
    ));
    let stream = tiny_stream();
    let root_transform =
        Transform::from_xyz(4.0, 2.0, -3.0).with_rotation(Quat::from_rotation_z(0.7));
    let root = app
        .world_mut()
        .spawn((
            stream,
            root_transform,
            Visibility::Inherited,
            Name::new("GaussianCloud: test"),
        ))
        .id();
    app.update();
    let old = app.world().get::<Children>(root).unwrap()[0];
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(old);
    app.update();
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_visible()
    );
    // Force another selection submission without network activity.
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .submitted
        .clear();
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs(1));
    app.update();
    let pending = app
        .world()
        .get::<Children>(root)
        .unwrap()
        .iter()
        .find(|entity| *entity != old)
        .unwrap();
    assert_eq!(
        *app.world().get::<Visibility>(pending).unwrap(),
        Visibility::Hidden
    );
    assert!(app.world().get_entity(old).is_ok());
    app.update();
    assert!(app.world().get_entity(old).is_ok());
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs(1));
    app.update();
    assert!(app.world().get_entity(old).is_err());
    let snapshot_transform = app.world().get::<GlobalTransform>(pending).unwrap();
    assert!(
        snapshot_transform
            .affine()
            .abs_diff_eq(GlobalTransform::from(root_transform).affine(), 1e-6,)
    );
    assert!(
        app.world()
            .get::<InheritedVisibility>(pending)
            .unwrap()
            .get()
    );
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, With<crate::streaming::runtime::SogStream>>();
    assert_eq!(query.iter(app.world()).collect::<Vec<_>>(), [root]);
    assert_eq!(
        *app.world().get::<Visibility>(pending).unwrap(),
        Visibility::Inherited
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_visible()
    );
    assert!(
        !app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    app.world_mut().despawn(root);
    app.update();
    assert!(app.world().get_entity(pending).is_err());
}

/// Replay the user's captured responses through the production decoder and scheduler.
/// Extracted HAR responses are deliberately not committed (large binary assets).
#[test]
#[ignore = "requires SOG_REPLAY_DIR with extracted superspl.at HAR responses"]
fn captured_scene_progressively_decodes_and_refines_under_budget() {
    let root = std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").unwrap());
    let mut stream = SogStream::parse(
        SCENE,
        "https://capture/lod-meta.json",
        "capture".into(),
        "capture".into(),
    )
    .unwrap();
    // The HAR covers a camera path, not all 46 files. Uncaptured files behave as
    // exhausted requests, exercising fallback without accessing the network.
    for (i, path) in stream.manifest.filenames.iter().enumerate() {
        if !root.join(path).is_file() {
            stream.entries[i].attempts = 3;
        }
    }
    let settings = SogLodSettings::default();
    let mut requested = Vec::new();
    let mut snapshots = 0;
    for tick in 0..100 {
        let changed = stream.tick(
            Vec3::new(-2.01, 0.127, -6.347).into(),
            tick as f64,
            &settings,
            gpu_splat_limit(128 * 1024 * 1024),
            None,
            |url, expected, _, now| {
                let path = url.strip_prefix("https://capture/").unwrap();
                requested.push(path.to_string());
                let meta_path = root.join(path);
                let meta_bytes = std::fs::read(&meta_path).unwrap();
                let meta = bevy_gaussian_splatting::io::sog::parse_meta(&meta_bytes).unwrap();
                if let Some(count) = expected {
                    assert_eq!(meta.count, count);
                }
                let files: HashMap<_, _> = meta
                    .referenced_files()
                    .iter()
                    .map(|name| {
                        (
                            name.to_string(),
                            std::fs::read(meta_path.parent().unwrap().join(name)).unwrap(),
                        )
                    })
                    .collect();
                ChunkLoad::captured(&meta_bytes, files, now)
            },
        );
        if changed {
            let cloud = compose::compose(&stream.active, |file| {
                stream
                    .entries
                    .get(file)
                    .and_then(|entry| entry.ready.as_ref())
            })
            .unwrap();
            assert!(cloud.len() <= settings.splat_budget + 31);
            snapshots += 1;
        }
        if !stream.pending_work {
            break;
        }
    }
    eprintln!(
        "requested={requested:?}; snapshots={snapshots}; splats={}",
        stream.active.iter().map(|r| r.count).sum::<usize>()
    );
    assert_eq!(requested[0], "5_0/meta.json");
    assert!(requested.iter().any(|p| p == "4_0/meta.json"));
    assert!(requested.iter().any(|p| p.starts_with("3_")));
    assert!(snapshots >= 3);
    assert!(
        stream
            .active
            .iter()
            .any(|r| Some(r.file) == stream.environment_index())
    );
    assert!(stream.active.iter().map(|r| r.count).sum::<usize>() > 406873);
    assert!(stream.last_error.is_none(), "{:?}", stream.last_error);
}

#[test]
fn gpu_limit_rejects_coarse_coverage_before_allocating_or_downloading() {
    let mut stream = tiny_stream();
    stream.manifest.leaves[0].lods[0].1.count = 1_000_000;
    stream.manifest.file_counts[0] = 1_000_000;
    let settings = SogLodSettings {
        cache_bytes: 512 * 1024 * 1024,
        ..default()
    };
    let limit = gpu_splat_limit(128 * 1024 * 1024);
    assert_eq!(limit % 32, 0);
    assert!(!stream.tick(
        Vec3::ZERO.into(),
        0.0,
        &settings,
        limit,
        None,
        |_, _, _, _| panic!("oversized coarse cloud must not download")
    ));
    assert!(
        stream
            .last_error
            .as_ref()
            .unwrap()
            .contains("GPU storage binding limit")
    );
}

fn stream_app(settings: SogLodSettings) -> (App, Entity, Entity) {
    let mut app = App::new();
    renderer::readiness::install(&mut app);
    app.insert_resource(renderer::SogRendererBackend::Snapshot);
    app.init_resource::<Assets<bevy_gaussian_splatting::sort::SortedEntries>>()
        .init_resource::<Time<Real>>()
        .insert_resource(settings)
        .init_resource::<Assets<PlanarGaussian3d>>()
        .add_systems(Update, update_sog_streams);
    let camera = app
        .world_mut()
        .spawn((
            StreamingCamera,
            Camera3d::default(),
            Projection::default(),
            GlobalTransform::IDENTITY,
        ))
        .id();
    let root = app
        .world_mut()
        .spawn((
            tiny_stream(),
            GlobalTransform::from(Transform::from_xyz(0., 0., -4.)),
        ))
        .id();
    (app, root, camera)
}

fn activate_snapshot(app: &mut App, root: Entity) -> Entity {
    app.update();
    let pending = app.world().get::<Children>(root).unwrap()[0];
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    app.update();
    pending
}

fn advance_stream(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs(1));
    app.update();
}

#[test]
fn streaming_uses_live_main_camera_fov_not_the_ui_camera() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert(GlobalTransform::from_translation(Vec3::new(6., 0., -10.)));
    app.world_mut().spawn((Camera2d, GlobalTransform::IDENTITY));
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .diagnostics = Some(Vec::new());
    let perspective = |degrees: f32| {
        Projection::Perspective(PerspectiveProjection {
            fov: degrees.to_radians(),
            aspect_ratio: 1.0,
            ..default()
        })
    };
    app.world_mut().entity_mut(camera).insert(perspective(20.));
    activate_snapshot(&mut app, root);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .diagnostics
            .as_ref()
            .unwrap()[0]
            .culled
    );
    app.world_mut().entity_mut(camera).insert(perspective(120.));
    advance_stream(&mut app);
    assert!(
        !app.world()
            .get::<SogStream>(root)
            .unwrap()
            .diagnostics
            .as_ref()
            .unwrap()[0]
            .culled
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .active
            .len(),
        1,
        "changing the main camera FOV must update visibility without reloading the scene"
    );
    // Narrowing again stops refinement eligibility, not coarse coverage.
    app.world_mut().entity_mut(camera).insert(perspective(20.));
    advance_stream(&mut app);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .diagnostics
            .as_ref()
            .unwrap()[0]
            .culled
    );
}

#[test]
fn lod_wireframe_follows_render_ready_selection_not_pending_refinement() {
    let (mut app, root, _) = stream_app(default());
    app.update();
    let stream = app.world().get::<SogStream>(root).expect("stream");
    assert!(
        stream.displayed.is_empty(),
        "initial upload is not displayed yet"
    );
    let coarse = stream.submitted.clone();
    assert_eq!(stream.status().displayed, 0);
    activate_snapshot(&mut app, root);
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        coarse
    );
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).expect("stream");
        // A finer range of the same leaf; both sources resident, but the next
        // render generation still needs its asynchronous readiness signal.
        stream.manifest.file_counts.push(2);
        stream.manifest.filenames.push("fine/meta.json".into());
        stream.manifest.leaves[0].lods[0].0 = 1;
        stream.manifest.leaves[0].lods.insert(
            0,
            (
                0,
                SplatRange {
                    file: 1,
                    offset: 0,
                    count: 2,
                },
            ),
        );
        stream.entries.push(Entry::default());
        stream.entries[1].ready = Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); 2].into(),
            count: 2,
        });
    }
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).expect("stream");
    assert_ne!(stream.submitted, coarse);
    assert_eq!(stream.status().selected, 2);
    assert_eq!(
        stream.status().displayed,
        1,
        "status must report ready rather than submitted detail"
    );
    assert_eq!(
        stream.displayed, coarse,
        "pending detail must not recolour boxes"
    );
    let pending = app.world().get::<Children>(root).expect("snapshots")[1];
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .expect("ready lock")
        .insert(pending);
    app.update();
    let stream = app.world().get::<SogStream>(root).expect("stream");
    assert_eq!(stream.displayed, stream.submitted);
    assert_eq!(stream.displayed[0].file, 1);
    assert_eq!(stream.status().displayed, 2);
    // Looking away requests coarse coverage, but retains the ready fine
    // generation until its coarse replacement is acknowledged.
    app.world_mut().resource_mut::<SogLodSettings>().visibility = SogVisibilityPolicy::Frustum;
    *app.world_mut()
        .get_mut::<GlobalTransform>(root)
        .expect("transform") = GlobalTransform::from(Transform::from_xyz(0., 0., 4.));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).expect("stream");
    assert_eq!(stream.submitted, coarse);
    assert_eq!(stream.displayed[0].file, 1);
    assert!(stream.snapshot.is_visible());
}

#[test]
fn cache_accounting_counts_shared_padded_files_once_and_separates_inflight_and_eviction() {
    let mut stream = tiny_stream();
    stream.entries[0].ready = None;
    assert_eq!(
        (
            stream.status().cache_ready_bytes,
            stream.status().cache_reserved_bytes
        ),
        (0, 0)
    );
    stream.entries[0].loading = Some(failed_captured_chunk(0.));
    stream.entries[0].reserved_splats = Some(32);
    let bytes = std::mem::size_of::<Gaussian3d>() as u64;
    // Aligned decoder input still reserves its appended full tile.
    assert_eq!(
        (
            stream.status().cache_ready_bytes,
            stream.status().cache_reserved_bytes
        ),
        (0, 64 * bytes)
    );
    stream.entries[0].complete(
        Ok(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); 64].into(),
            count: 32,
        }),
        0.,
    );
    stream.submitted = vec![
        SplatRange {
            file: 0,
            offset: 0,
            count: 8,
        },
        SplatRange {
            file: 0,
            offset: 8,
            count: 8,
        },
    ];
    assert_eq!(
        (
            stream.status().cache_ready_bytes,
            stream.status().cache_reserved_bytes
        ),
        (64 * bytes, 0)
    );
    stream.entries.push(Entry {
        ready: Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); 32].into(),
            count: 1,
        }),
        ..default()
    });
    stream.entries.push(Entry::default());
    assert_eq!(stream.status().cache_ready_bytes, 96 * bytes);
    assert!(cache::admit(
        &mut stream.entries,
        &[64, 32, 32],
        &[true, false, true],
        2,
        96
    ));
    assert!(stream.entries[0].ready.is_some());
    assert!(stream.entries[1].ready.is_none());
    assert_eq!(stream.status().cache_ready_bytes, 64 * bytes);
}

#[test]
fn failed_replacement_keeps_render_ready_generation() {
    let (mut app, root, _) = stream_app(default());
    let old = activate_snapshot(&mut app, root);
    let original = vec![SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    }];
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        stream.manifest.leaves[0].lods[0].1.count = 2;
        stream.manifest.file_counts[0] = 2;
        // A genuine adapter validation failure, without a GPU/device or mock.
    }
    advance_stream(&mut app);
    assert!(app.world().get_entity(old).is_ok());
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    assert!(
        !app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .loading_task()
            .is_some_and(|task| task.state == ProgressTaskState::Error)
    );
    // The same failure on another tick must keep its error and old visible child.
    advance_stream(&mut app);
    assert!(app.world().get_entity(old).is_ok());
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .loading_task()
            .is_some_and(|task| task.state == ProgressTaskState::Error)
    );
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        let chunk = stream.entries[0].ready.as_mut().unwrap();
        chunk.cloud = vec![Gaussian3d::default(); 2].into();
        chunk.count = 2;
    }
    advance_stream(&mut app);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    assert_eq!(app.world().get::<Children>(root).unwrap().len(), 2);
    assert!(app.world().get_entity(old).is_ok());
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    let pending = app
        .world()
        .get::<Children>(root)
        .expect("children")
        .iter()
        .find(|entity| *entity != old)
        .expect("replacement snapshot");
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Hidden)
    );
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .expect("readiness")
        .insert(pending);
    app.update();
    assert!(app.world().get_entity(old).is_err());
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        [SplatRange {
            file: 0,
            offset: 0,
            count: 2
        }]
    );
}

#[test]
fn offscreen_coarse_snapshot_stays_render_ready_without_resubmission() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(stream.active.len(), 1);
    assert!(stream.snapshot.is_visible());
    assert!(!stream.snapshot.is_pending());
    assert!(app.world().get_entity(old).is_ok());
    assert!(stream.entries[0].ready.is_some());
    assert!(stream.loading_task().is_none());
}

#[test]
fn exact_gpu_source_union_admits_refinement_without_half_capacity_tax() {
    let mut stream = tiny_stream();
    // The parsed manifest cannot contain two entries at LOD 0. Give coarse
    // coverage its real band before adding a distinct finer representation.
    stream.manifest.leaves[0].lods[0].0 = 1;
    stream.manifest.filenames.push("fine/meta.json".into());
    stream.manifest.file_counts.push(100);
    stream.manifest.leaves[0].lods.insert(
        0,
        (
            0,
            SplatRange {
                file: 1,
                offset: 0,
                count: 100,
            },
        ),
    );
    stream.entries.push(Entry::default());
    // Coarse (1) + fine (100) fits the GPU cap of 180; no half-capacity tax.
    let settings = SogLodSettings {
        cache_bytes: 400 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    };
    let mut requests = Vec::new();
    stream.tick(
        Vec3::ZERO.into(),
        0.,
        &settings,
        1000,
        Some((180 * chunk::DecodedChunk::bytes_per_splat()) as u64),
        |url, _, _, now| {
            requests.push(url);
            failed_captured_chunk(now)
        },
    );
    assert_eq!(requests, ["https://example.test/fine/meta.json"]);
    assert_eq!(
        stream.active,
        [SplatRange {
            file: 0,
            offset: 0,
            count: 1
        }]
    );
}

#[test]
fn resident_source_rejection_after_new_download_reservation_reaches_quality_summary() {
    for tracing in [false, true] {
        let mut stream = SogStream::parse(
            br#"{"lodLevels":3,"filenames":["coarse/meta.json","middle/meta.json","fine/meta.json"],
                "tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{
                    "0":{"file":2,"offset":0,"count":8},
                    "1":{"file":1,"offset":0,"count":6},
                    "2":{"file":0,"offset":0,"count":4}}}}"#,
            "https://example.test/lod-meta.json", "test".into(), "test".into(),
        ).unwrap();
        assert_eq!(stream.manifest.leaves[0].lods.len(), 3);
        for (file, count) in [(0, 4), (1, 6)] {
            stream.entries[file].ready = Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); count].into(),
                count,
            });
        }
        stream.configure_diagnostics(tracing);
        let mut requests = Vec::new();
        // Desired coarse+fine uses 12 GPU rows after replacing the unpinned
        // middle source. Starting that download pins fine before the resident
        // pass: coarse+fine+middle would then cost 18, so middle must be rejected.
        stream.tick(
            Vec3::ZERO.into(),
            0.,
            &SogLodSettings::default(),
            1000,
            Some(12 * renderer::source_bytes_per_splat()),
            |url, _, _, now| {
                requests.push(url);
                failed_captured_chunk(now)
            },
        );
        assert_eq!(requests, ["https://example.test/fine/meta.json"]);
        assert!(
            stream.entries[1].ready.is_some(),
            "CPU cache must retain the candidate"
        );
        assert!(
            stream.entries[2].loading.is_some(),
            "new reservation must still be pinned"
        );
        assert_eq!(
            stream.active,
            [SplatRange {
                file: 0,
                offset: 0,
                count: 4
            }]
        );
        assert!(stream.last_error.is_none());
        assert!(
            stream.admission.source_limited,
            "resident rejection was lost (tracing={tracing})"
        );
        assert!(!stream.admission.selection_limited);
        assert!(
            stream
                .status()
                .limiting_reason
                .contains("source-transition candidates rejected")
        );
        assert_eq!(stream.diagnostics.is_some(), tracing);

        // Conversely, keep a desired-only count rejection when the resident
        // pass has no finer available candidate (and clear the prior source flag).
        stream.entries[1].ready = None;
        stream.entries[2].loading = None;
        stream.entries[2].reserved_splats = None;
        let settings = SogLodSettings {
            splat_budget: 4,
            ..default()
        };
        stream.tick(
            Vec3::ZERO.into(),
            1.,
            &settings,
            1000,
            Some(12 * renderer::source_bytes_per_splat()),
            |_, _, _, _| panic!("rejected candidate must not download"),
        );
        assert!(stream.admission.selection_limited);
        assert!(!stream.admission.source_limited);
        assert!(
            stream
                .status()
                .limiting_reason
                .contains("candidates exceeded selection budget")
        );
    }
}

#[test]
fn camera_turn_reuses_cached_coarse_coverage_while_finer_chunk_fails() {
    let mut stream = SogStream::parse(
        br#"{
        "lodLevels":2,"filenames":["coarse/meta.json","left/meta.json","right/meta.json"],
        "tree":{"bound":{"min":[-10,-1,-1],"max":[10,1,1]},"children":[
            {"bound":{"min":[-10,-1,-1],"max":[-8,1,1]},"lods":{
                "0":{"file":1,"offset":0,"count":8},"1":{"file":0,"offset":0,"count":2}}},
            {"bound":{"min":[8,-1,-1],"max":[10,1,1]},"lods":{
                "0":{"file":2,"offset":0,"count":8},"1":{"file":0,"offset":2,"count":2}}}
        ]}}"#,
        "https://example.test/lod-meta.json",
        "test".into(),
        "test".into(),
    )
    .unwrap();
    stream.entries[0].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default(); 4].into(),
        count: 4,
    });
    let settings = SogLodSettings {
        splat_budget: 10,
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    };
    let left = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(-10., -8., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let right = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(8., 10., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let mut requests = Vec::new();
    stream.tick(left, 0., &settings, 1000, None, |url, _, _, now| {
        requests.push(url);
        failed_captured_chunk(now)
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 0,
                offset: 2,
                count: 2
            }
        ]
    );
    assert_eq!(requests, ["https://example.test/left/meta.json"]);
    // Exhaust the failed finer file. Its fallback remains visible and coarse stays resident.
    stream.entries[1].loading = None;
    stream.entries[1].attempts = 3;
    stream.tick(left, 10., &settings, 1000, None, |_, _, _, _| {
        panic!("exhausted file must not retry")
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 0,
                offset: 2,
                count: 2
            }
        ]
    );
    stream.tick(right, 20., &settings, 1000, None, |url, _, _, now| {
        requests.push(url);
        failed_captured_chunk(now)
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 0,
                offset: 2,
                count: 2
            }
        ]
    );
    assert_eq!(requests[1], "https://example.test/right/meta.json");
    assert!(stream.entries[0].ready.is_some());
    stream.entries[2].loading = None;
    stream.entries[2].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default(); 8].into(),
        count: 8,
    });
    stream.tick(right, 30., &settings, 1000, None, |_, _, _, _| {
        panic!("cached sources must not redownload")
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 2,
                offset: 0,
                count: 8
            }
        ]
    );
}

/// Exercise the real completed-download error path, without issuing HTTP requests.
fn failed_captured_chunk(now: f64) -> ChunkLoad {
    ChunkLoad::captured(
        br#"{
        "version":2,"count":8,
        "means":{"mins":[-1,-1,-1],"maxs":[1,1,1],"files":["lo.webp","hi.webp"]},
        "scales":{"codebook":[0],"files":["scales.webp"]},
        "quats":{"files":["quats.webp"]},
        "sh0":{"codebook":[0],"files":["sh0.webp"]}
    }"#,
        HashMap::new(),
        now,
    )
}

#[test]
fn all_direction_coarse_coverage_never_waives_gpu_binding_limit() {
    let mut stream = tiny_stream();
    stream.manifest.leaves.push(manifest::Leaf {
        bound: manifest::Bounds {
            min: Vec3::new(8., -1., -1.),
            max: Vec3::new(10., 1., 1.),
        },
        lods: vec![(
            0,
            SplatRange {
                file: 0,
                offset: 1,
                count: 2,
            },
        )],
    });
    stream.manifest.file_counts[0] = 3;
    stream.entries[0].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default(); 3].into(),
        count: 3,
    });
    let view = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(-2., 2., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let settings = SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    };
    stream.tick(view, 0., &settings, 1, None, |_, _, _, _| {
        panic!("resident source must not download")
    });
    assert!(stream.active.is_empty());
    assert_eq!(
        stream.last_error.as_deref(),
        Some("Coarsest SOG coverage exceeds the GPU storage binding limit")
    );
}

#[test]
fn camera_turn_preserves_current_and_unready_pending_coverage() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .submitted
        .clear();
    advance_stream(&mut app);
    let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
    assert_eq!(children.len(), 2);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(!stream.active.is_empty());
    assert!(stream.snapshot.is_visible());
    assert!(stream.snapshot.is_pending());
    assert!(app.world().get_entity(old).is_ok());
    assert!(
        children
            .into_iter()
            .all(|entity| app.world().get_entity(entity).is_ok())
    );
}

#[test]
fn environment_remains_selected_when_all_tree_leaves_are_offscreen() {
    let mut stream = tiny_stream();
    stream.manifest.environment = Some("environment/meta.json".into());
    stream.entries.push(Entry {
        ready: Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default()].into(),
            count: 1,
        }),
        ..default()
    });
    let view = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(8., 10., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let settings = SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    };
    stream.tick(view, 0., &settings, 1000, None, |_, _, _, _| {
        panic!("resident sources must not download")
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 1
            },
            SplatRange {
                file: 1,
                offset: 0,
                count: 1
            }
        ]
    );
    assert!(stream.entries[0].ready.is_some());
}

#[test]
fn offscreen_coverage_stays_ready_with_unrenderable_declared_environment() {
    for (state, entry) in [
        (
            "unready",
            Entry {
                attempts: 1,
                retry_at: 1000.,
                ..default()
            },
        ),
        (
            "failed",
            Entry {
                attempts: 3,
                retry_at: 7.,
                ..default()
            },
        ),
        (
            "zero",
            Entry {
                ready: Some(chunk::DecodedChunk {
                    cloud: default(),
                    count: 0,
                }),
                ..default()
            },
        ),
    ] {
        let (mut app, root, camera) = stream_app(SogLodSettings {
            visibility: SogVisibilityPolicy::Frustum,
            ..default()
        });
        let old = activate_snapshot(&mut app, root);
        app.world_mut()
            .get_mut::<SogStream>(root)
            .unwrap()
            .submitted
            .clear();
        advance_stream(&mut app);
        let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
        assert_eq!(children.len(), 2);
        {
            let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
            stream.manifest.environment = Some("environment/meta.json".into());
            stream.entries.push(entry);
        }
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert!(!stream.active.is_empty(), "{state}");
        assert!(stream.snapshot.is_visible(), "{state}");
        assert!(stream.snapshot.is_pending(), "{state}");
        assert!(app.world().get_entity(old).is_ok(), "{state}");
        assert!(
            children
                .iter()
                .all(|entity| app.world().get_entity(*entity).is_ok()),
            "{state}"
        );
        // Transport retry scheduling resumes after the renderer acknowledges
        // the pending coverage. Merely turning away must not cancel that wait.
        let pending = *children.iter().find(|&&entity| entity != old).unwrap();
        app.world()
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .insert(pending);
        app.update();
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        let environment = &stream.entries[1];
        match state {
            "unready" => {
                assert_eq!(environment.attempts, 1);
                assert_eq!(environment.retry_at, 1000.);
                assert!(stream.pending_work);
            }
            "failed" => {
                assert_eq!(environment.attempts, 3);
                assert_eq!(environment.retry_at, 7.);
                assert!(!stream.pending_work);
            }
            "zero" => {
                assert_eq!(environment.ready.as_ref().unwrap().count, 0);
                assert!(!stream.pending_work);
            }
            _ => unreachable!(),
        }
        assert!(
            stream.entries[0].ready.is_some(),
            "{state}: coarse cache must stay ready"
        );
    }
}

#[test]
fn ready_nonempty_environment_retains_pending_generations_when_tree_is_offscreen() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        stream.manifest.environment = Some("environment/meta.json".into());
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default()].into(),
                count: 1,
            }),
            ..default()
        });
    }
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .submitted
        .clear();
    advance_stream(&mut app);
    let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
    assert_eq!(children.len(), 2);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(stream.snapshot.is_visible());
    assert!(stream.snapshot.is_pending());
    assert!(app.world().get_entity(old).is_ok());
    assert!(
        children
            .into_iter()
            .all(|entity| app.world().get_entity(entity).is_ok())
    );
    assert!(
        stream
            .active
            .iter()
            .any(|range| range.file == 1 && range.count == 1)
    );
}
