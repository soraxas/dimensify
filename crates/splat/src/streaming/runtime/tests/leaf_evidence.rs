//! Leaf diagnostics and LOD wireframe evidence follow the real scheduler.
use super::*;

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
