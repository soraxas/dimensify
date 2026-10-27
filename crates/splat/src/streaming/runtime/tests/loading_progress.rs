//! Ready transport must progress without waiting for policy or GPU replacement.
use super::*;
use base64::Engine;

fn completed_refinement(app: &mut App, root: Entity) -> usize {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/worker-expanded-v2.json")).unwrap();
    let files = fixture["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, encoded)| {
            (
                name.clone(),
                base64::engine::general_purpose::STANDARD
                    .decode(encoded.as_str().unwrap())
                    .unwrap(),
            )
        })
        .collect();
    let load = ChunkLoad::captured(&serde_json::to_vec(&fixture["meta"]).unwrap(), files, 0.);
    let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
    let file = stream.entries.len();
    stream
        .manifest
        .filenames
        .push(format!("fine-{file}/meta.json"));
    stream.manifest.file_counts.push(2);
    stream.entries.push(Entry {
        loading: Some(load),
        reserved_splats: Some(2),
        attempts: 1,
        ..default()
    });
    // An independent refinement can be downloading while the current snapshot
    // waits for readiness. Its admission must not change renderer ownership.
    stream.pending_work = true;
    file
}

fn next_frame(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(16));
    app.update();
}

#[test]
fn activation_trace_waits_for_renderer_acknowledgement() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let trace = crate::diagnostics::sog_cache::SogCacheTrace::enabled();
    app.insert_resource(trace.clone());
    app.update();
    let report = serde_json::to_value(trace.report().unwrap()).unwrap();
    let events = report["activation"]["recent"]
        .as_array()
        .expect("enabled telemetry must report activation stages");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["kind"], "target");
    assert_eq!(events[1]["kind"], "submitted");
    assert_eq!(events[0]["availability"], "unclassified_backend");
    assert_eq!(events[0]["range"]["file"], 0);
    assert_eq!(events[0]["range"]["offset"], 0);
    assert_eq!(events[0]["range"]["count"], 1);
    let pending = app.world().get::<Children>(root).unwrap()[0];
    next_frame(&mut app);
    assert_eq!(
        serde_json::to_value(trace.report().unwrap()).unwrap()["activation"]["recent"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    next_frame(&mut app);
    let report = serde_json::to_value(trace.report().unwrap()).unwrap();
    let events = report["activation"]["recent"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[2]["kind"], "activated");
    assert_eq!(events[2]["episode"], events[0]["episode"]);
    assert_eq!(events[2]["scene"], root.to_bits().to_string());
    assert!(events[2]["elapsed_ms"].as_f64().unwrap() >= events[1]["elapsed_ms"].as_f64().unwrap());
}

fn requested_refinement(app: &mut App, root: Entity) -> SplatRange {
    let file = completed_refinement(app, root);
    let fine = SplatRange {
        file,
        offset: 0,
        count: 2,
    };
    let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
    stream.manifest.leaves[0].lods[0].0 = 5;
    stream.manifest.leaves[0].lods.insert(0, (0, fine));
    stream.overlay_intent.queued_targets = vec![Some(fine)];
    let current = stream.active.clone();
    let SogStream {
        replacements,
        manifest,
        ..
    } = &mut *stream;
    replacements.reconcile(manifest, &[Some(fine)], &current);
    replacements.requests = vec![0, file];
    fine
}

#[test]
fn completed_requested_detail_submits_before_periodic_deadline() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let visible = activate_snapshot(&mut app, root);
    let fine = requested_refinement(&mut app, root);
    next_frame(&mut app); // admit the worker result, bounded to one chunk
    next_frame(&mut app); // consume the completion wake, not the 200ms timer
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.submitted,
        [fine],
        "ready detail must not wait for periodic selection"
    );
    assert!(stream.snapshot.is_pending());
    assert_ne!(stream.displayed, [fine], "submission is not activation");
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn ready_replacement_retries_when_admission_recovers_before_policy_deadline() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let visible = activate_snapshot(&mut app, root);
    let fine = requested_refinement(&mut app, root);
    let trace = crate::diagnostics::sog_cache::SogCacheTrace::enabled();
    app.insert_resource(trace.clone());
    let (generation, deadline) = {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        stream.cache_trace = trace.scene(root);
        let SogStream {
            activation,
            cache_trace,
            generation,
            manifest,
            displayed,
            entries,
            ..
        } = &mut *stream;
        activation.observe(
            cache_trace,
            *generation,
            manifest,
            &[Some(fine)],
            displayed,
            entries,
        );
        (*generation, stream.next_update)
    };
    // The source finishes, but the live old+new CPU union cannot fit yet.
    app.world_mut().resource_mut::<SogLodSettings>().cache_bytes =
        32 * chunk::DecodedChunk::bytes_per_splat();
    next_frame(&mut app);
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(stream.entries[fine.file].ready.is_some());
    assert_ne!(stream.submitted, [fine]);
    assert!(
        trace
            .report()
            .unwrap()
            .activation
            .recent
            .iter()
            .any(|event| {
                event.guard.is_some_and(|guard| {
                    guard.reason == crate::diagnostics::sog_cache::ActivationGuard::SourceAdmission
                })
            }),
        "the actual failed admission branch must be observed"
    );

    // Holding the same pressure must not retry admission on every frame.
    for _ in 0..3 {
        next_frame(&mut app);
        assert!(
            !app.world()
                .get::<SogStream>(root)
                .unwrap()
                .replacements
                .dirty
        );
    }
    assert_eq!(
        trace
            .report()
            .unwrap()
            .activation
            .recent
            .iter()
            .filter(|event| {
                event.guard.is_some_and(|guard| {
                    guard.reason == crate::diagnostics::sog_cache::ActivationGuard::SourceAdmission
                })
            })
            .count(),
        1,
        "unchanged pressure must not repeatedly resolve and reject the same target"
    );
    app.world_mut().resource_mut::<SogLodSettings>().cache_bytes =
        SogLodSettings::default().cache_bytes;
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.submitted,
        [fine],
        "cleared admission must not wait for the camera-policy deadline"
    );
    assert_eq!(stream.generation, generation);
    assert_eq!(stream.next_update, deadline);
    assert_ne!(stream.displayed, [fine]);
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn replacement_admission_wakes_only_for_changed_live_limits() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    activate_snapshot(&mut app, root);
    requested_refinement(&mut app, root);
    let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
    let mut settings = SogLodSettings::default();
    let mut gpu_limit = 1000;
    let mut gpu_bytes = None;
    stream.refresh_replacement_admission(&settings, gpu_limit, gpu_bytes);
    for input in [
        "cpu",
        "splats",
        "gpu_count",
        "gpu_bytes",
        "gpu_bytes_removed",
    ] {
        stream.replacements.dirty = false;
        stream.refresh_replacement_admission(&settings, gpu_limit, gpu_bytes);
        assert!(!stream.replacements.dirty, "unchanged {input}");
        match input {
            "cpu" => settings.cache_bytes /= 2,
            "splats" => settings.splat_budget /= 2,
            "gpu_count" => gpu_limit /= 2,
            "gpu_bytes" => gpu_bytes = Some(1024),
            _ => gpu_bytes = None,
        }
        stream.refresh_replacement_admission(&settings, gpu_limit, gpu_bytes);
        assert!(stream.replacements.dirty, "changed {input}");
    }
}

#[test]
fn renderer_acknowledgement_retries_ready_detail_before_policy_deadline() {
    let settings = SogLodSettings {
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    };
    let (mut app, root, _) = stream_app(settings.clone());
    app.update();
    let pending = app.world().get::<Children>(root).unwrap()[0];
    let fine = requested_refinement(&mut app, root);
    next_frame(&mut app); // completion while the coarse bridge awaits acknowledgement
    next_frame(&mut app);
    let (retired, generation, deadline) = {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        // A previous displayed fine source still owns memory while its covered
        // coarse bridge is pending. The next fine target is already ready.
        let retired = SplatRange {
            file: stream.entries.len(),
            offset: 0,
            count: 1,
        };
        stream.manifest.filenames.push("retired/meta.json".into());
        stream.manifest.file_counts.push(1);
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); 32].into(),
                count: 1,
            }),
            ..default()
        });
        stream.displayed = vec![retired];
        assert!(stream.entries[fine.file].ready.is_some());
        assert!(!stream.progress_replacements(&settings, usize::MAX, None));
        assert!(!stream.replacements.dirty);
        (retired, stream.generation, stream.next_update)
    };
    next_frame(&mut app);
    assert_eq!(
        app.world().get::<SogStream>(root).unwrap().displayed,
        [retired]
    );
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.submitted,
        [fine],
        "acknowledgement must release the blocked union"
    );
    assert_ne!(
        stream.displayed,
        [fine],
        "the new replacement still needs its own acknowledgement"
    );
    assert_eq!(stream.generation, generation);
    assert_eq!(stream.next_update, deadline);
    assert!(stream.snapshot.is_pending());
}

#[test]
fn due_policy_keeps_just_completed_detail_while_next_target_loads() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let visible = activate_snapshot(&mut app, root);
    let middle = requested_refinement(&mut app, root);
    let finest = {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        let file = stream.entries.len();
        let finest = SplatRange {
            file,
            offset: 0,
            count: 4,
        };
        stream.manifest.filenames.push("finest/meta.json".into());
        stream.manifest.file_counts.push(4);
        stream.entries.push(Entry {
            loading: Some(ChunkLoad::pending(0.)),
            reserved_splats: Some(4),
            attempts: 1,
            ..default()
        });
        stream.manifest.leaves[0].lods[0].0 = 2;
        stream.manifest.leaves[0].lods.insert(0, (0, finest));
        stream.next_update = 0.;
        finest
    };

    next_frame(&mut app);

    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(
        stream.replacements.wants(finest.file),
        "policy still chooses the latest target"
    );
    assert_eq!(
        stream.submitted,
        [middle],
        "resolve accepted A→B before policy chooses C; retain B while C loads"
    );
    assert!(stream.snapshot.is_pending());
    assert_ne!(
        stream.displayed,
        [middle],
        "completion is not renderer acknowledgement"
    );
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn completion_preserves_policy_deadline_and_does_not_resample_camera() {
    let (mut app, root, camera) = stream_app(SogLodSettings::default());
    activate_snapshot(&mut app, root);
    let fine = requested_refinement(&mut app, root);
    let (generation, deadline) = {
        let stream = app.world().get::<SogStream>(root).unwrap();
        (stream.generation, stream.next_update)
    };
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(1000., 0., 0.)));
    next_frame(&mut app);
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.generation, generation,
        "completion is not a policy tick"
    );
    assert_eq!(stream.next_update, deadline);
    assert_eq!(
        stream.submitted,
        [fine],
        "resolve the accepted target, not a new camera target"
    );
}

#[test]
fn late_superseded_completion_cannot_activate_over_the_latest_leaf_target() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let visible = activate_snapshot(&mut app, root);
    let obsolete = requested_refinement(&mut app, root);
    let latest = requested_refinement(&mut app, root);
    let generation = app.world().get::<SogStream>(root).unwrap().generation;
    next_frame(&mut app); // B finishes, but only C owns the replacement intent.
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(stream.entries[obsolete.file].ready.is_some());
    assert_ne!(stream.submitted, [obsolete]);
    assert!(!stream.snapshot.is_pending());
    next_frame(&mut app); // C finishes and can submit without a policy tick.
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(stream.submitted, [latest]);
    assert_eq!(stream.generation, generation);
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn freed_download_slot_dispatches_current_queue_without_policy_evaluation() {
    let mut stream = tiny_stream();
    for i in 1..=2 {
        stream
            .manifest
            .filenames
            .push(format!("fine-{i}/meta.json"));
        stream.manifest.file_counts.push(2);
        stream.entries.push(Entry::default());
    }
    stream.entries[1].loading = Some(failed_captured_chunk(0.));
    stream.entries[1].reserved_splats = Some(2);
    stream.entries[1].attempts = 1;
    stream.replacements.requests = vec![0, 2]; // B is obsolete; C waits for its slot.
    stream.generation = 9;
    stream.next_update = 100.;
    let settings = SogLodSettings {
        concurrent_chunks: 1,
        ..default()
    };
    let mut started = Vec::new();
    stream.progress_requests(0., &settings, None, |url, _, _, now| {
        started.push(url);
        failed_captured_chunk(now)
    });
    assert!(started.is_empty());
    stream.poll_loads(1.);
    stream.progress_requests(1., &settings, None, |url, _, _, now| {
        started.push(url);
        failed_captured_chunk(now)
    });
    assert_eq!(started.len(), 1);
    assert!(started[0].ends_with("fine-2/meta.json"));
    assert!(stream.entries[2].loading.is_some());
    assert_eq!(stream.entries[1].attempts, 1, "do not retry obsolete work");
    assert_eq!(stream.generation, 9);
    assert_eq!(stream.next_update, 100.);
}

#[test]
fn queued_dispatch_respects_reduced_cpu_and_gpu_headroom() {
    for gpu in [false, true] {
        let mut stream = tiny_stream();
        stream.manifest.filenames.push("incoming/meta.json".into());
        stream.manifest.file_counts.push(2);
        stream.entries.push(Entry::default());
        stream.replacements.requests = vec![0, 1];
        let settings = SogLodSettings {
            cache_bytes: if gpu {
                1024 * 1024
            } else {
                32 * chunk::DecodedChunk::bytes_per_splat()
            },
            ..default()
        };
        let gpu_bytes = gpu.then_some(2 * renderer::source_bytes_per_splat());
        stream.progress_requests(0., &settings, gpu_bytes, |_, _, _, _| {
            panic!("union does not fit")
        });
        assert!(
            stream.entries[0].ready.is_some(),
            "rejection preserves coverage"
        );
        assert!(stream.entries[1].loading.is_none());
    }
}

#[test]
fn pending_downgrade_retains_old_detail_until_its_target_is_ready() {
    let mut stream = pixel_stream();
    let old = stream.active.clone();
    let middle = stream.manifest.leaves[0].lods[1].1;
    stream.entries[middle.file].ready = None;
    let settings = SogLodSettings {
        splat_budget: 100_000,
        ..default()
    };
    let view = selection::View {
        pixels: Some(screen_space::PixelView {
            clip_from_local: Mat4::orthographic_rh(-2.5, 2.5, -2.5, 2.5, 0.1, 1000.),
            viewport: Vec2::splat(200.),
        }),
        ..Vec3::ZERO.into()
    };
    let mut requested = Vec::new();
    stream.tick(view, 0., &settings, 100_000, None, |url, _, _, now| {
        requested.push(url);
        failed_captured_chunk(now)
    });
    assert_eq!(requested, ["https://example.test/middle/meta.json"]);
    assert_eq!(
        stream.active, old,
        "no coarse flash while a replacement is pending"
    );
    assert!(stream.entries[middle.file].loading.is_some());
}

#[test]
fn pending_policy_downgrade_cannot_fund_another_leaf_upgrade() {
    let mut stream = pixel_stream();
    let mut second = manifest::Leaf {
        bound: stream.manifest.leaves[0].bound,
        lods: stream.manifest.leaves[0].lods.clone(),
    };
    second.bound.min *= 2.;
    second.bound.max *= 2.;
    for (_, range) in &mut second.lods {
        range.file += 3;
    }
    stream.manifest.leaves.push(second);
    for (name, count) in [
        ("other-fine", 10000),
        ("other-middle", 2500),
        ("other-coarse", 625),
    ] {
        stream.manifest.filenames.push(format!("{name}/meta.json"));
        stream.manifest.file_counts.push(count);
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                count,
                cloud: vec![Gaussian3d::default(); count].into(),
            }),
            ..default()
        });
    }
    stream.active.push(stream.manifest.leaves[1].lods[1].1);
    stream.submitted = stream.active.clone();
    stream.displayed = stream.active.clone();
    let old = stream.active.clone();
    stream.entries[1].ready = None;
    let settings = SogLodSettings {
        splat_budget: 12500,
        ..default()
    };
    let view = selection::View {
        pixels: Some(screen_space::PixelView {
            clip_from_local: Mat4::orthographic_rh(-2.5, 2.5, -2.5, 2.5, 0.1, 1000.),
            viewport: Vec2::splat(200.),
        }),
        ..Vec3::ZERO.into()
    };
    stream.tick(view, 0., &settings, 12500, None, |_, _, _, now| {
        failed_captured_chunk(now)
    });
    assert!(
        stream.entries[1].loading.is_some(),
        "first leaf's middle target is pending"
    );
    assert_eq!(
        stream.active, old,
        "pending downgrade must keep its count before other upgrades"
    );
}

#[test]
fn reduced_count_cap_takes_precedence_over_pending_old_detail() {
    let stream = pixel_stream();
    let fine = stream.manifest.leaves[0].lods[0].1;
    let middle = stream.manifest.leaves[0].lods[1].1;
    let coarse = stream.manifest.leaves[0].lods[2].1;
    let selected = replacement::Replacements::policy_selection(
        &stream.manifest,
        &[Some(middle)],
        &[fine],
        vec![coarse],
        middle.count,
        |file| file != middle.file,
    );
    assert_eq!(
        selected,
        [coarse],
        "retain coverage, not an over-budget old selection"
    );
}

#[test]
fn accepted_queue_progresses_while_renderer_blocks_an_expired_policy_tick() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    app.update(); // An unacknowledged renderer generation owns the old coverage.
    let fine = requested_refinement(&mut app, root);
    let generation = {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        let entry = &mut stream.entries[fine.file];
        entry.loading = None;
        let mut load =
            raster_handoff_tests::chunk(Vec3::ZERO, true, 0.).with_storage(SourceStorage::Packed);
        entry.warm = Some(load.poll_measured(0., &default()).unwrap().unwrap());
        entry.reserved_splats = None;
        stream.storage = SourceStorage::Packed;
        stream.next_update = 0.;
        stream.generation
    };
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(
        stream.entries[fine.file].ready.is_some(),
        "accepted queue must progress despite render backpressure"
    );
    assert!(stream.entries[fine.file].warm.is_none());
    assert_eq!(
        stream.generation, generation,
        "do not replan while renderer owns the generation"
    );
    assert_ne!(
        stream.submitted,
        [fine],
        "transport progress is not renderer readiness"
    );
    assert!(stream.snapshot.is_pending());
}

#[test]
fn blocked_policy_deadline_runs_on_first_render_ready_frame() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    app.update();
    let pending = app.world().get::<Children>(root).unwrap()[0];
    let generation = app.world().get::<SogStream>(root).unwrap().generation;
    // The periodic deadline expires while the first generation is still pending.
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(210));
    app.update();
    assert_eq!(
        app.world().get::<SogStream>(root).unwrap().generation,
        generation
    );
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    next_frame(&mut app);
    assert_eq!(
        app.world().get::<SogStream>(root).unwrap().generation,
        generation + 1,
        "back pressure must defer a policy tick, not consume it"
    );
    // A readiness acknowledgement is not permission to evaluate every camera frame.
    next_frame(&mut app);
    assert_eq!(
        app.world().get::<SogStream>(root).unwrap().generation,
        generation + 1
    );
}

#[test]
fn completion_wake_survives_pending_renderer_generation() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    app.update();
    let pending = app.world().get::<Children>(root).unwrap()[0];
    let fine = requested_refinement(&mut app, root);
    next_frame(&mut app);
    next_frame(&mut app);
    assert_eq!(app.world().get::<Children>(root).unwrap().len(), 1);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .displayed
            .is_empty()
    );
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.submitted,
        [fine],
        "completion wake must survive GPU back pressure"
    );
    assert_ne!(stream.displayed, [fine]);
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn failed_requested_detail_keeps_backoff_and_does_not_wake_policy() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    activate_snapshot(&mut app, root);
    let fine = requested_refinement(&mut app, root);
    app.world_mut().get_mut::<SogStream>(root).unwrap().entries[fine.file].loading =
        Some(failed_captured_chunk(0.));
    let generation = app.world().get::<SogStream>(root).unwrap().generation;
    next_frame(&mut app);
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(stream.generation, generation);
    assert!(stream.entries[fine.file].ready.is_none());
    assert!(stream.entries[fine.file].loading.is_none());
    assert!(stream.entries[fine.file].retry_at > 1.);
}

#[test]
fn camera_movement_alone_keeps_periodic_policy_cadence() {
    let (mut app, root, camera) = stream_app(SogLodSettings::default());
    activate_snapshot(&mut app, root);
    let generation = app.world().get::<SogStream>(root).unwrap().generation;
    for frame in 1..8 {
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from_translation(Vec3::new(
                frame as f32 * 0.1,
                0.,
                0.,
            )));
        next_frame(&mut app);
        assert_eq!(
            app.world().get::<SogStream>(root).unwrap().generation,
            generation
        );
    }
}

#[test]
fn unused_gpu_cache_eviction_does_not_trigger_camera_replanning() {
    let (mut app, root, camera) = stream_app(SogLodSettings::default());
    activate_snapshot(&mut app, root);
    let (generation, deadline);
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        generation = stream.generation;
        deadline = stream.next_update;
        // A formerly used fine file survives only in the cache, not in the
        // active/submitted/displayed selection or required coarse coverage.
        stream.manifest.filenames.push("cached/meta.json".into());
        stream.manifest.file_counts.push(2);
        stream.manifest.leaves[0].lods[0].0 = 5;
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
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); 2].into(),
                count: 2,
            }),
            attempts: 3,
            retry_at: 10.,
            ..default()
        });
        stream.sync_resident_sources(|file, ready| {
            if file == 1 {
                *ready = None;
                true
            } else {
                false
            }
        });
        assert!(stream.entries[1].ready.is_none());
        assert_eq!(stream.entries[1].attempts, 0);
        assert_eq!(stream.entries[1].retry_at, 0.);
    }
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(1000., 0., 0.)));
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.generation, generation,
        "unused cached detail does not invalidate owned coverage"
    );
    assert_eq!(stream.next_update, deadline);
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(200));
    app.update();
    assert_eq!(
        app.world().get::<SogStream>(root).unwrap().generation,
        generation + 1,
        "ordinary camera policy must still run when due"
    );
}

#[test]
fn eviction_of_owned_or_required_sources_still_wakes_policy() {
    for owner in ["active", "submitted", "displayed", "coarse", "environment"] {
        let mut stream = tiny_stream();
        let range = SplatRange {
            file: 1,
            offset: 0,
            count: 1,
        };
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default()].into(),
                count: 1,
            }),
            ..default()
        });
        if owner == "environment" {
            stream.manifest.environment = Some("env/meta.json".into());
        } else {
            stream.manifest.filenames.push("extra/meta.json".into());
            stream.manifest.file_counts.push(1);
        }
        match owner {
            "active" => stream.active = vec![range],
            "submitted" => stream.submitted = vec![range],
            "displayed" => stream.displayed = vec![range],
            "coarse" => stream.manifest.leaves[0].lods[0].1 = range,
            _ => (),
        }
        stream.next_update = 10.;
        stream.sync_resident_sources(|file, ready| {
            if file == 1 {
                *ready = None;
                true
            } else {
                false
            }
        });
        assert_eq!(
            stream.next_update, 0.,
            "lost {owner} source needs prompt policy recovery"
        );
        assert!(stream.entries[1].ready.is_none());
    }
}

#[test]
fn covered_bridge_resumes_accepted_requests_without_another_policy_tick() {
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
            |_, _, _, _| panic!("left source is resident"),
        );
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        stream.tick(
            Vec3::new(9., 0., 0.).into(),
            1.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("old sources still own the headroom"),
        );
        assert_eq!(
            stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );
        let generation = stream.generation;
        stream.next_update = 1.2;
        // Submission alone cannot release the old displayed generation's pins.
        stream.submitted = stream.active.clone();
        stream.progress_requests(1.01, &settings, gpu_bytes, |_, _, _, _| {
            panic!("bridge is not render-ready yet")
        });
        assert!(stream.entries[1].ready.is_some());
        assert!(stream.entries[2].loading.is_none());

        // Acknowledge the covered generation before the 200ms policy deadline.
        stream.displayed = stream.submitted.clone();
        // Accepted intent does not reserve future headroom. A reduced CPU cap
        // or another scene's GPU pins must still prevent acquisition.
        let mut reduced = settings.clone();
        if !gpu_pressure {
            reduced.cache_bytes = 32 * chunk::DecodedChunk::bytes_per_splat();
        }
        let reduced_gpu = gpu_pressure.then_some(4 * renderer::source_bytes_per_splat());
        stream.progress_requests(1.015, &reduced, reduced_gpu, |_, _, _, _| {
            panic!("current headroom overrides retained intent")
        });
        let mut started = Vec::new();
        stream.progress_requests(1.02, &settings, gpu_bytes, |url, _, _, now| {
            started.push(url);
            failed_captured_chunk(now)
        });
        assert_eq!(
            started,
            ["https://example.test/right/meta.json"],
            "bridge acknowledgement must resume accepted intent, GPU pressure={gpu_pressure}"
        );
        assert_eq!(stream.generation, generation);
        assert_eq!(stream.next_update, 1.2);
        assert_eq!(
            stream.displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );
        if !gpu_pressure {
            assert!(
                stream.entries[1].ready.is_none(),
                "retired CPU source yields to replacement"
            );
        }
        // Complete the admitted source through the real entry lifecycle. The
        // accepted fine range must replace coarse coverage without replanning.
        assert!(
            stream.entries[2]
                .complete(
                    Ok(chunk::DecodedChunk {
                        cloud: vec![Gaussian3d::default(); 32].into(),
                        count: 8,
                    }),
                    1.03
                )
                .is_none()
        );
        assert!(stream.progress_replacements(&settings, 1000, gpu_bytes));
        assert_eq!(
            stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(stream.active.iter().map(|r| r.count).sum::<usize>(), 10);
        assert_eq!(
            stream.displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0],
            "preparing detail must not replace displayed ownership"
        );
        assert_eq!(stream.generation, generation);
        assert_eq!(stream.next_update, 1.2);
    }
}

#[test]
fn resident_replacement_checks_actual_union_not_future_destination() {
    for (gpu_pressure, reduced_cap) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut stream = relocation_stream();
        let mut settings = SogLodSettings {
            splat_budget: 10,
            cache_bytes: (if gpu_pressure { 256 } else { 96 })
                * chunk::DecodedChunk::bytes_per_splat(),
            ..default()
        };
        let mut gpu_bytes = gpu_pressure.then_some(20 * renderer::source_bytes_per_splat());
        stream.tick(
            Vec3::new(-9., 0., 0.).into(),
            0.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("left detail is resident"),
        );
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        let old_displayed = stream.displayed.clone();
        // An obsolete load retains its reservation. It leaves enough memory
        // for current owners, but not the entire newly desired destination.
        stream.manifest.filenames.push("obsolete/meta.json".into());
        stream.manifest.file_counts.push(8);
        stream.entries.push(Entry {
            loading: Some(ChunkLoad::pending(0.)),
            reserved_splats: Some(8),
            attempts: 1,
            ..default()
        });
        if reduced_cap {
            if gpu_pressure {
                gpu_bytes = Some(12 * renderer::source_bytes_per_splat());
            } else {
                settings.cache_bytes = 64 * chunk::DecodedChunk::bytes_per_splat();
            }
        }
        stream.tick(
            Vec3::new(9., 0., 0.).into(),
            1.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("there is no headroom for another source"),
        );
        assert!(stream.overlay_intent.transition_blocked);
        assert!(
            stream.relocation_files.as_ref().unwrap()[2],
            "right detail is desired"
        );
        if reduced_cap {
            assert_eq!(
                stream.active, old_displayed,
                "even resident changes must respect the live union"
            );
        } else {
            assert_eq!(
                stream
                    .active
                    .iter()
                    .map(|range| (range.file, range.offset, range.count))
                    .collect::<Vec<_>>(),
                [(0, 0, 2), (0, 2, 2)],
                "ready left downgrade must not wait for the unrelated source; gpu={gpu_pressure}"
            );
        }
        assert_eq!(
            stream.displayed, old_displayed,
            "selection is not renderer acknowledgement"
        );
        assert_eq!(stream.submitted, old_displayed);
        assert!(
            stream.entries[3].loading.is_some(),
            "obsolete reservation stays owned"
        );
        assert!(
            stream.entries[2].loading.is_none(),
            "no new source was admitted"
        );
        if reduced_cap {
            continue;
        }
        let generation = stream.generation;
        stream.progress_requests(1.01, &settings, gpu_bytes, |_, _, _, _| {
            panic!("old displayed ownership still prevents acquisition")
        });
        // Renderer acknowledgement, not selecting the downgrade, frees the
        // old source. The existing accepted queue can now acquire the target.
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        let mut requests = Vec::new();
        stream.progress_requests(1.02, &settings, gpu_bytes, |url, _, _, now| {
            requests.push(url);
            ChunkLoad::pending(now)
        });
        assert_eq!(requests, ["https://example.test/right/meta.json"]);
        assert!(
            stream.entries[3].loading.is_some(),
            "unrelated load need not finish first"
        );
        assert!(stream.entries[2].loading.is_some());
        assert_eq!(stream.generation, generation, "no extra camera-policy pass");
    }
}

#[test]
fn independent_resident_replacement_submits_without_releasing_displayed_ownership() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        splat_budget: 10,
        cache_bytes: 96 * chunk::DecodedChunk::bytes_per_splat(),
        concurrent_chunks: 1,
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert((relocation_stream(), GlobalTransform::IDENTITY));
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(-9., 0., 0.)));
    let visible = activate_snapshot(&mut app, root);
    let old_displayed = {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        stream.manifest.filenames.push("obsolete/meta.json".into());
        stream.manifest.file_counts.push(8);
        stream.entries.push(Entry {
            loading: Some(ChunkLoad::pending(0.)),
            reserved_splats: Some(8),
            attempts: 1,
            ..default()
        });
        stream.displayed.clone()
    };
    assert_eq!(
        old_displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
        [1, 0]
    );
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(9., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream
            .submitted
            .iter()
            .map(|r| (r.file, r.offset, r.count))
            .collect::<Vec<_>>(),
        [(0, 0, 2), (0, 2, 2)],
        "the system must submit independent ready progress"
    );
    assert!(stream.snapshot.is_pending());
    assert_eq!(stream.displayed, old_displayed);
    assert!(stream.entries[3].loading.is_some());
    assert!(stream.entries[2].loading.is_none());
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn due_policy_supersedes_the_destination_retained_through_a_bridge() {
    let mut stream = relocation_stream();
    let settings = SogLodSettings {
        splat_budget: 10,
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    };
    stream.tick(
        Vec3::new(-9., 0., 0.).into(),
        0.,
        &settings,
        1000,
        None,
        |_, _, _, _| panic!("left source is resident"),
    );
    stream.submitted = stream.active.clone();
    stream.displayed = stream.active.clone();
    stream.tick(
        Vec3::new(9., 0., 0.).into(),
        1.,
        &settings,
        1000,
        None,
        |_, _, _, _| panic!("bridge must retire old sources first"),
    );
    assert_eq!(
        stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
        [0, 0]
    );
    stream.submitted = stream.active.clone();
    stream.displayed = stream.active.clone();

    // The bridge finishes after a new camera-policy evaluation becomes due.
    // Returning left supersedes the unstarted right request; no frozen view.
    stream.tick(
        Vec3::new(-9., 0., 0.).into(),
        2.,
        &settings,
        1000,
        None,
        |_, _, _, _| panic!("returning left needs no download"),
    );
    stream.progress_requests(2.01, &settings, None, |_, _, _, _| {
        panic!("obsolete right target must not dispatch")
    });
    assert_eq!(
        stream.active.iter().map(|r| r.file).collect::<Vec<_>>(),
        [1, 0]
    );
    assert_eq!(stream.entries[2].attempts, 0);
}

#[test]
fn due_policy_supersedes_destination_while_obsolete_transport_keeps_its_reservation() {
    for gpu_pressure in [false, true] {
        let mut stream = relocation_stream();
        let settings = SogLodSettings {
            splat_budget: 10,
            cache_bytes: (if gpu_pressure { 256 } else { 64 })
                * chunk::DecodedChunk::bytes_per_splat(),
            ..default()
        };
        let gpu_bytes = gpu_pressure.then_some(12 * renderer::source_bytes_per_splat());
        for (time, x) in [(0., -9.), (1., 9.)] {
            stream.tick(
                Vec3::new(x, 0., 0.).into(),
                time,
                &settings,
                1000,
                gpu_bytes,
                |_, _, _, _| panic!("resident selection or covered bridge"),
            );
            stream.submitted = stream.active.clone();
            stream.displayed = stream.active.clone();
        }
        stream.progress_requests(1.01, &settings, gpu_bytes, |url, _, _, now| {
            assert!(url.ends_with("right/meta.json"));
            ChunkLoad::pending(now)
        });
        assert!(stream.entries[2].loading.is_some());
        let reservation = stream.entries[2].reserved_splats;
        assert_eq!(reservation, Some(8));

        // Camera returns while right is genuinely unfinished, not failed.
        // Intent may change; old transport's memory charge may not disappear.
        stream.tick(
            Vec3::new(-9., 0., 0.).into(),
            2.,
            &settings,
            1000,
            gpu_bytes,
            |_, _, _, _| panic!("obsolete reservation still consumes headroom"),
        );
        assert_eq!(
            stream.relocation_files,
            Some(vec![true, true, false]),
            "latest camera must own destination under CPU/GPU pressure"
        );
        assert!(stream.entries[2].loading.is_some());
        assert_eq!(stream.entries[2].reserved_splats, reservation);
        assert!(
            !stream.replacements.wants(2),
            "late right result cannot activate"
        );
        stream.progress_requests(2.01, &settings, gpu_bytes, |_, _, _, _| {
            panic!("new intent must not bypass outstanding byte reservations")
        });
        assert_eq!(
            stream.displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
            [0, 0]
        );

        // Late obsolete completion is cache data, not permission to display it.
        assert!(
            stream.entries[2]
                .complete(
                    Ok(chunk::DecodedChunk {
                        cloud: vec![Gaussian3d::default(); 32].into(),
                        count: 8,
                    }),
                    2.02
                )
                .is_none()
        );
        assert!(!stream.replacements.wants(2));
        stream.tick(
            Vec3::new(-9., 0., 0.).into(),
            3.,
            &settings,
            1000,
            gpu_bytes,
            |url, _, _, now| {
                assert!(
                    url.ends_with("left/meta.json"),
                    "only latest intent may start"
                );
                ChunkLoad::pending(now)
            },
        );
        assert!(stream.active.iter().all(|range| range.file != 2));
        assert!(
            stream.entries[1].ready.is_some() || stream.entries[1].loading.is_some(),
            "latest destination must progress once obsolete transport releases its reservation"
        );
    }
}

#[test]
fn rejected_bridge_keeps_retry_throttled_and_old_coverage_visible() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        splat_budget: 10,
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert((relocation_stream(), GlobalTransform::IDENTITY));
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(-9., 0., 0.)));
    let old = activate_snapshot(&mut app, root);
    // Genuine snapshot composition rejection: the already-rendered old asset
    // survives, but a future bridge cannot read its invalid coarse source range.
    app.world_mut().get_mut::<SogStream>(root).unwrap().entries[0]
        .ready
        .as_mut()
        .unwrap()
        .count = 1;
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(9., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(
        stream.submission_error.is_some(),
        "fixture must reject the bridge"
    );
    assert!(!stream.snapshot.is_pending());
    let generation = stream.generation;
    for _ in 0..6 {
        next_frame(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert_eq!(
            stream.generation, generation,
            "failed bridges must not replan each frame"
        );
        assert_eq!(stream.entries[2].attempts, 0);
        assert_eq!(
            stream.displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
            [1, 0]
        );
        assert_eq!(
            app.world().get::<Visibility>(old),
            Some(&Visibility::Inherited)
        );
    }
}

#[test]
fn admits_completed_source_before_next_lod_deadline_without_replanning() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let visible = activate_snapshot(&mut app, root);
    let file = completed_refinement(&mut app, root);
    let generation = app.world().get::<SogStream>(root).unwrap().generation;
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(
        stream.entries[file].ready.is_some(),
        "ready data must not wait 200ms"
    );
    assert_eq!(stream.entries[file].ready.as_ref().unwrap().count, 2);
    assert!(stream.entries[file].loading.is_none());
    assert!(stream.entries[file].reserved_splats.is_none());
    assert_eq!(stream.generation, generation, "policy remains throttled");
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn admits_completed_source_while_replacement_is_not_render_ready() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    app.update();
    let pending = app.world().get::<Children>(root).unwrap()[0];
    let file = completed_refinement(&mut app, root);
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .next_update = 0.;
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(
        stream.entries[file].ready.is_some(),
        "GPU readiness must not block transport"
    );
    assert!(stream.snapshot.is_pending());
    assert_eq!(app.world().get::<Children>(root).unwrap()[0], pending);
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Hidden)
    );
    assert!(
        stream.displayed.is_empty(),
        "loading is not renderer acknowledgement"
    );
}

#[test]
fn completion_admission_remains_one_chunk_per_frame() {
    for policy_due in [false, true] {
        let (mut app, root, _) = stream_app(SogLodSettings::default());
        activate_snapshot(&mut app, root);
        let first = completed_refinement(&mut app, root);
        let second = completed_refinement(&mut app, root);
        if policy_due {
            app.world_mut()
                .get_mut::<SogStream>(root)
                .unwrap()
                .next_update = 0.;
        }
        next_frame(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert!(stream.entries[first].ready.is_some());
        assert!(
            stream.entries[second].loading.is_some(),
            "bound main-thread completion work even on a due policy tick"
        );
        next_frame(&mut app);
        assert!(
            app.world().get::<SogStream>(root).unwrap().entries[second]
                .ready
                .is_some()
        );
    }
}

#[test]
fn partial_upload_retries_next_frame_without_replanning_or_retiring_visible_scene() {
    let (mut app, root, _) = stream_app(SogLodSettings::default());
    let visible = activate_snapshot(&mut app, root);
    let generation;
    let candidate;
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        generation = stream.generation;
        // An already decoded replacement differs from the visible source.
        stream
            .manifest
            .filenames
            .push("replacement/meta.json".into());
        stream.manifest.file_counts.push(1);
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default()].into(),
                count: 1,
            }),
            ..default()
        });
        stream.active[0].file = 1;
        candidate = stream.active.clone();
        stream.preparing_upload = true;
        stream.next_update = 100.;
    }
    next_frame(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream.generation, generation,
        "upload retry must not replan"
    );
    assert_eq!(
        stream.submitted, candidate,
        "retry must use pinned candidate"
    );
    assert!(!stream.preparing_upload);
    assert!(
        stream.snapshot.is_pending(),
        "upload completion still waits for render readiness"
    );
    assert_eq!(
        app.world().get::<Visibility>(visible),
        Some(&Visibility::Inherited)
    );
}

#[test]
fn unfinished_upload_is_busy_even_after_downloads_and_policy_settle() {
    let mut stream = tiny_stream();
    stream.generation = 1;
    stream.pending_work = false;
    stream.preparing_upload = true;
    let mut activity = crate::diagnostics::SogPerfActivity::default();
    stream.accumulate_perf_activity(&mut activity);
    assert_eq!(
        activity.pending_generations, 1,
        "an upload must keep the redraw chain awake"
    );
    assert!(stream.status().pending_generation);
}
