use super::*;
use crate::renderer::{compose, gpu_splat_limit, readiness::ReadySnapshots};
use crate::streaming::SogLodPolicy;
use bevy_gaussian_splatting::{Gaussian3d, Planar, PlanarGaussian3d};
use std::{collections::HashMap, time::Duration};

mod captured_scene;
mod environment_coverage;
mod generation_handoff;
mod leaf_evidence;
mod loading_progress;
mod pressure_lifecycle;
mod relocation_lifecycle;
mod screen_space_policy;
mod source_admission;

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

mod scheduling;

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
