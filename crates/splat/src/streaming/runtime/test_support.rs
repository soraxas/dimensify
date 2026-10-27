//! Deterministic acceptance fixtures; excluded from production graphs.
use super::*;
use crate::renderer::readiness::ReadySnapshots;
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d};
use std::time::Duration;
/// Drive a deterministic streaming acceptance fixture.
pub fn tiny_stream() -> SogStream {
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

/// Drive a deterministic streaming acceptance fixture.
pub fn stream_app(settings: SogLodSettings) -> (App, Entity, Entity) {
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

/// Drive a deterministic streaming acceptance fixture.
pub fn activate_snapshot(app: &mut App, root: Entity) -> Entity {
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

/// Drive a deterministic streaming acceptance fixture.
pub fn advance_stream(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs(1));
    app.update();
}

/// Replace a fixture source while retaining the displayed generation.
/// Drive a deterministic streaming acceptance fixture.
pub fn resize_source(stream: &mut SogStream, count: usize) {
    stream.manifest.file_counts[0] = count;
    stream.manifest.leaves[0].lods[0].1.count = count;
    stream.entries[0].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default(); (count / 32 + 1) * 32].into(),
        count,
    });
}
/// Create independent per-scene selection evidence for aggregation tests.
/// Drive a deterministic streaming acceptance fixture.
pub fn capped_stream(cap: usize) -> SogStream {
    let mut stream = tiny_stream();
    stream.effective = cap;
    stream.submitted = vec![SplatRange {
        file: 0,
        offset: 0,
        count: cap,
    }];
    stream
}

/// Inject bounded activity evidence for application presentation acceptance.
pub fn activity(stream: &mut SogStream, effective: usize, pending: bool, limited: bool) {
    stream.generation = 1;
    stream.requested = 8;
    stream.effective = effective;
    stream.pending_work = pending;
    stream.admission.selection_limited = limited;
}
/// Mark the deterministic fixture as performing a bounded GPU upload.
pub fn uploading(stream: &mut SogStream, value: bool) {
    stream.preparing_upload = value;
}
/// Select the authored level used by the overlay inventory fixture.
pub fn level(stream: &mut SogStream, value: usize) {
    stream.manifest.leaves[0].lods[0].0 = value;
}
