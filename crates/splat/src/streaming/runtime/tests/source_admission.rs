//! Source/GPU admission budgets, coverage floors and cache accounting.
use super::*;

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
