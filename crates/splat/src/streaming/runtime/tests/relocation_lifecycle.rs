//! Source relocation between detailed regions under cache and GPU pressure.
use super::*;

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
