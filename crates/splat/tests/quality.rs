#![cfg(feature = "streaming")]

use bevy::prelude::*;
use dimensify_splat::{
    diagnostics::quality::{
        SogDiagnosticSettings, SogLeafSnapshot, SogQualityReport, SplatDiagnosticsPlugin,
        SplatDiagnosticsSet,
    },
    streaming::{
        SogLodSettings,
        runtime::{SogStream, diagnostics::SogLeafDiagnostic},
    },
};

const MANIFEST: &[u8] = br#"{"lodLevels":1,"filenames":["coarse/meta.json"],"tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{"0":{"file":0,"offset":0,"count":1}}}}"#;

fn stream() -> SogStream {
    SogStream::parse(
        MANIFEST,
        "https://example.test/lod-meta.json",
        "fixture".into(),
        "fixture".into(),
    )
    .unwrap()
}

#[test]
fn report_observes_stream_admission_and_unload_without_mutating_requested_quality() {
    let mut app = App::new();
    app.insert_resource(SogLodSettings {
        splat_budget: 3_500_000,
        ..default()
    })
    .add_plugins(SplatDiagnosticsPlugin);
    let entity = app.world_mut().spawn(stream()).id();
    app.update();
    let report = app.world().resource::<SogQualityReport>();
    assert!(report.available);
    assert_eq!(report.requested, 3_500_000);
    assert_eq!(report.activity, "Evaluating");
    assert_eq!(
        report.displayed, 0,
        "a requested stream is not rendered evidence"
    );
    if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
        assert!(
            report
                .memory_summary
                .contains("GPU estimates include padded attribute textures")
        );
    } else {
        assert!(
            report.memory_summary.contains("not measured allocation")
                || report.memory_summary.contains("Allocated slots")
        );
    }
    assert_eq!(
        app.world().resource::<SogLodSettings>().splat_budget,
        3_500_000
    );
    app.world_mut().despawn(entity);
    app.update();
    let report = app.world().resource::<SogQualityReport>();
    assert!(!report.available);
    assert_eq!(
        (report.effective, report.selected, report.displayed),
        (0, 0, 0)
    );
}

#[test]
fn diagnostic_snapshot_is_bounded_and_opt_out_releases_leaf_evidence() {
    let mut app = App::new();
    app.add_plugins(SplatDiagnosticsPlugin);
    app.world_mut()
        .resource_mut::<SogDiagnosticSettings>()
        .enabled = true;
    let entities: Vec<_> = (0..5)
        .map(|_| app.world_mut().spawn(stream()).id())
        .collect();
    app.update();
    for entity in &entities {
        let mut stream = app.world_mut().get_mut::<SogStream>(*entity).unwrap();
        assert!(stream.diagnostics.is_some());
        stream.diagnostics = Some(
            (0..300)
                .map(|leaf| SogLeafDiagnostic {
                    leaf,
                    projected_bounds: None,
                    culled: false,
                    desired_lod: Some(0),
                    chosen_lod: None,
                    range: None,
                    reason: "not requested",
                })
                .collect(),
        );
    }
    app.update();
    let snapshot = app.world().resource::<SogLeafSnapshot>();
    assert_eq!(snapshot.scene_count, 5);
    assert_eq!(snapshot.scenes.len(), 4);
    assert_eq!(
        snapshot
            .scenes
            .iter()
            .map(|scene| scene.leaves.len())
            .sum::<usize>(),
        256
    );
    assert_eq!(serde_json::to_value(snapshot).unwrap()["scene_count"], 5);
    app.world_mut()
        .resource_mut::<SogDiagnosticSettings>()
        .enabled = false;
    app.update();
    assert!(app.world().resource::<SogLeafSnapshot>().scenes.is_empty());
    assert!(entities.iter().all(|entity| {
        app.world()
            .get::<SogStream>(*entity)
            .unwrap()
            .diagnostics
            .is_none()
    }));
}

#[derive(Resource, Default)]
struct Publications(usize);

#[test]
fn unchanged_observations_do_not_publish_redundant_changes() {
    let mut app = App::new();
    app.add_plugins(SplatDiagnosticsPlugin)
        .init_resource::<Publications>()
        .add_systems(
            PostUpdate,
            (|report: Res<SogQualityReport>, mut count: ResMut<Publications>| {
                if report.is_changed() {
                    count.0 += 1;
                }
            })
            .after(SplatDiagnosticsSet::Publish),
        );
    app.update();
    app.update();
    assert_eq!(app.world().resource::<Publications>().0, 1);
    app.world_mut()
        .resource_mut::<SogLodSettings>()
        .splat_budget = 2_000_000;
    app.update();
    assert_eq!(app.world().resource::<Publications>().0, 2);
}

#[test]
#[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
fn per_scene_backend_override_is_reported_instead_of_global_default() {
    let mut app = App::new();
    app.insert_resource(dimensify_splat::renderer::SogRendererBackend::Packed)
        .add_plugins(SplatDiagnosticsPlugin);
    app.world_mut().spawn((
        stream(),
        dimensify_splat::runtime::SogStreamRenderSettings::new(
            dimensify_splat::renderer::SogRendererBackend::Snapshot,
        ),
    ));
    app.update();
    let report = app.world().resource::<SogQualityReport>();
    assert!(report.memory_summary.contains("Snapshot GPU estimate"));
    assert!(!report.sh_description.contains("packed source bands"));
}

#[test]
#[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
fn different_projection_layouts_do_not_publish_one_shared_selection_limit() {
    let mut app = App::new();
    app.add_plugins(SplatDiagnosticsPlugin);
    app.world_mut()
        .resource_mut::<SogDiagnosticSettings>()
        .enabled = true;
    for projection_cache in [false, true] {
        app.world_mut().spawn((
            stream(),
            dimensify_splat::runtime::SogStreamRenderSettings::new(
                dimensify_splat::renderer::SogRendererBackend::Packed,
            )
            .with_projection_cache(projection_cache),
        ));
    }
    app.update();
    let snapshot = app.world().resource::<SogLeafSnapshot>();
    assert_eq!(snapshot.scenes.len(), 2);
    assert!(
        snapshot.selection_limits.is_none(),
        "different layouts must not share one capacity"
    );
    assert_eq!(
        snapshot.backend, "packed",
        "a layout difference is not a backend difference"
    );
    let mut layouts: Vec<_> = snapshot
        .scenes
        .iter()
        .map(|scene| scene.selection_limits.projection_cache)
        .collect();
    layouts.sort_unstable();
    assert_eq!(layouts, [false, true]);
}
