//! Captured-scene fixtures and replays through the production decoder and scheduler.
use super::*;

const SCENE: &[u8] = include_bytes!("../fixtures/sanatorium-lod.json");

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
