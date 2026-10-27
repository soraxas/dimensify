//! Request-order regressions exercise the real selector, admission and queue.
//! Only external transport is replaced with a captured failing response.
use super::*;

fn stream() -> SogStream {
    let mut stream = relocation_stream();
    stream.entries[1].ready = None;
    stream.manifest.leaves.push(manifest::Leaf {
        bound: manifest::Bounds {
            min: Vec3::new(20., -1., -1.),
            max: Vec3::new(22., 1., 1.),
        },
        lods: vec![
            (
                0,
                SplatRange {
                    file: 3,
                    offset: 0,
                    count: 8,
                },
            ),
            (
                1,
                SplatRange {
                    file: 0,
                    offset: 4,
                    count: 2,
                },
            ),
        ],
    });
    stream.manifest.filenames.push("third/meta.json".into());
    stream.manifest.file_counts = vec![6, 8, 8, 8];
    stream.entries.push(default());
    stream.entries[0].ready.as_mut().expect("coarse").count = 6;
    stream.active = stream
        .manifest
        .leaves
        .iter()
        .map(|leaf| leaf.lods[1].1)
        .collect();
    stream.submitted = stream.active.clone();
    stream.displayed = stream.active.clone();
    stream
}

fn settings() -> SogLodSettings {
    SogLodSettings {
        splat_budget: 24,
        cache_bytes: 128 * chunk::DecodedChunk::bytes_per_splat(),
        concurrent_chunks: 1,
        ..default()
    }
}

fn request(stream: &mut SogStream, eye: Vec3, now: f64) -> Vec<String> {
    let mut requests = Vec::new();
    stream.tick(
        eye.into(),
        now,
        &settings(),
        64,
        Some(30 * renderer::source_bytes_per_splat()),
        |url, _, _, now| {
            requests.push(url);
            failed_captured_chunk(now)
        },
    );
    let status = stream.status();
    assert!(
        status.cache_ready_bytes + status.cache_reserved_bytes <= settings().cache_bytes as u64
    );
    assert!(
        stream
            .entries
            .iter()
            .filter(|entry| entry.loading.is_some())
            .count()
            <= 1
    );
    assert_eq!(
        stream.active.len(),
        3,
        "coverage survives pending refinement"
    );
    requests
}

fn complete(stream: &mut SogStream, file: usize) {
    stream.entries[file].complete(
        Ok(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default(); 32].into(),
            count: stream.manifest.file_counts[file],
        }),
        0.,
    );
}

#[test]
fn nearby_refinement_starts_before_manifest_first_file() {
    let mut stream = stream();
    assert_eq!(
        request(&mut stream, Vec3::new(9., 0., 0.), 0.),
        ["https://example.test/right/meta.json"]
    );
    assert_eq!(stream.entries[1].attempts, 0);
    assert_eq!(stream.entries[2].attempts, 1);
}

#[test]
fn cache_history_distinguishes_first_request_reload_and_failed_retry() {
    use crate::diagnostics::sog_cache::{Kind, SogCacheTrace};
    let trace = SogCacheTrace::enabled();
    let mut world = World::new();
    let scene = world.spawn_empty().id();
    let mut stream = stream();
    stream.cache_trace = trace.scene(scene);
    let eye = Vec3::new(-9., 0., 0.);
    assert_eq!(request(&mut stream, eye, 0.).len(), 1);
    complete(&mut stream, 1);
    // Use real admission eviction (which must preserve successful-load history
    // but clear retry attempts), rather than manually resetting the entry.
    assert!(cache::admit_observed(
        &mut stream.entries,
        &[32; 4],
        &[true, false, true, false],
        2,
        64,
        |file, bytes| trace.scene(scene).record(
            Kind::CpuAdmissionEviction,
            file,
            bytes,
            Some(2),
            1
        ),
    ));
    assert_eq!(request(&mut stream, eye, 1.).len(), 1);
    // Captured transport failures flow through the real poll/backoff path.
    for now in 2..10 {
        request(&mut stream, eye, f64::from(now));
    }
    let kinds: Vec<_> = trace
        .report()
        .unwrap()
        .recent
        .iter()
        .filter(|event| event.source == 1)
        .map(|event| event.kind)
        .collect();
    assert_eq!(
        &kinds[..5],
        &[
            Kind::Request,
            Kind::CpuAdmissionEviction,
            Kind::Reload,
            Kind::DecodeFailed,
            Kind::Retry,
        ]
    );
}

#[test]
fn camera_move_reorders_unstarted_files_after_a_download_completes() {
    let mut stream = stream();
    assert_eq!(
        request(&mut stream, Vec3::new(-9., 0., 0.), 0.),
        ["https://example.test/left/meta.json"]
    );
    complete(&mut stream, 1);
    assert_eq!(
        request(&mut stream, Vec3::new(21., 0., 0.), 1.),
        ["https://example.test/third/meta.json"]
    );
    complete(&mut stream, 3);
    assert_eq!(
        request(&mut stream, Vec3::new(21., 0., 0.), 2.),
        ["https://example.test/right/meta.json"]
    );
    complete(&mut stream, 2);
    assert!(request(&mut stream, Vec3::new(21., 0., 0.), 3.).is_empty());
    assert!(stream.entries.iter().all(|entry| entry.attempts <= 1));
    assert!(!stream.pending_work);
}

#[test]
fn missing_displayed_coverage_beats_nearby_refinement() {
    let mut stream = stream();
    stream.displayed.remove(2);
    assert_eq!(
        request(&mut stream, Vec3::new(-9., 0., 0.), 0.),
        ["https://example.test/third/meta.json"]
    );
}

#[test]
fn failed_near_request_does_not_block_others_or_bypass_retry_limits() {
    let mut stream = stream();
    let eye = Vec3::new(9., 0., 0.);
    assert_eq!(
        request(&mut stream, eye, 0.),
        ["https://example.test/right/meta.json"]
    );
    assert_eq!(
        request(&mut stream, eye, 1.),
        ["https://example.test/third/meta.json"]
    );
    assert_eq!(
        stream.entries[2].attempts, 1,
        "near request remains in backoff"
    );
    for time in 2..40 {
        request(&mut stream, eye, f64::from(time));
    }
    assert!(stream.entries[1..].iter().all(|entry| entry.attempts == 3));
    assert!(request(&mut stream, eye, 100.).is_empty());
    assert!(!stream.pending_work);
}

#[test]
fn shared_file_uses_its_nearest_requesting_leaf_and_loads_once() {
    let mut stream = stream();
    stream.manifest.leaves[2].lods[0].1 = SplatRange {
        file: 1,
        offset: 8,
        count: 8,
    };
    stream.manifest.file_counts[1] = 16;
    stream.manifest.file_counts[3] = 0;
    let eye = Vec3::new(21., 0., 0.);
    // File 1's first leaf is far away, but its other leaf is at the eye.
    assert_eq!(
        request(&mut stream, eye, 0.),
        ["https://example.test/left/meta.json"]
    );
    complete(&mut stream, 1);
    assert_eq!(
        request(&mut stream, eye, 1.),
        ["https://example.test/right/meta.json"]
    );
    complete(&mut stream, 2);
    assert!(request(&mut stream, eye, 2.).is_empty());
    assert_eq!(stream.entries[1].attempts, 1);
    assert_eq!(stream.entries[3].attempts, 0);
}

#[test]
fn coarse_environment_and_missing_coverage_precede_refinement_with_stable_ties() {
    let mut stream = stream();
    stream.manifest.environment = Some("environment/meta.json".into());
    // Equal distances must retain request order instead of oscillating.
    for leaf in &mut stream.manifest.leaves {
        leaf.bound = manifest::Bounds {
            min: -Vec3::ONE,
            max: Vec3::ONE,
        };
    }
    let desired: Vec<_> = stream
        .manifest
        .leaves
        .iter()
        .map(|leaf| Some(leaf.lods[0].1))
        .collect();
    let queue = |displayed: &[SplatRange]| {
        super::super::scheduling::requests(
            &stream.manifest,
            &desired,
            displayed,
            Vec3::ZERO.into(),
            Some(4),
        )
    };
    assert_eq!(queue(&stream.displayed), [0, 4, 1, 2, 3]);
    assert_eq!(queue(&stream.displayed[..2]), [0, 4, 3, 1, 2]);
    assert_eq!(queue(&stream.displayed), [0, 4, 1, 2, 3]);
}

#[test]
fn camera_move_does_not_cancel_or_restart_an_inflight_download() {
    let mut stream = stream();
    let mut held = failed_captured_chunk(0.);
    // Transport stand-in: consume its captured completion so later polls yield
    // nothing. The test explicitly completes the admitted file after moving.
    assert!(held.poll_measured(0., &SogCpuTimings::default()).is_some());
    stream.entries[1].loading = Some(held);
    stream.entries[1].attempts = 1;
    stream.entries[1].reserved_splats = Some(8);
    assert!(request(&mut stream, Vec3::new(21., 0., 0.), 1.).is_empty());
    assert!(stream.entries[1].loading.is_some());
    assert_eq!(stream.entries[1].attempts, 1);
    complete(&mut stream, 1);
    assert_eq!(
        request(&mut stream, Vec3::new(21., 0., 0.), 2.),
        ["https://example.test/third/meta.json"]
    );
}

#[test]
fn zero_count_or_withdrawn_detail_does_not_enter_request_queue() {
    let stream = stream();
    let queue = super::super::scheduling::requests(
        &stream.manifest,
        &[
            None,
            Some(SplatRange {
                file: 2,
                offset: 0,
                count: 0,
            }),
            Some(SplatRange {
                file: 3,
                offset: 0,
                count: 8,
            }),
        ],
        &stream.displayed,
        Vec3::ZERO.into(),
        None,
    );
    assert_eq!(queue, [0, 3]);
}
