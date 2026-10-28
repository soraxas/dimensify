#[path = "pipeline/loading.rs"]
mod loading;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use dimensify_splat::streaming::{
    Manifest,
    runtime::test_support::benchmarks::{
        CacheReplay, PressureReplay, ResidentReplay, SelectionReplay, manifest_bytes,
    },
};
use std::{hint::black_box, time::Duration};

fn policies(c: &mut Criterion) {
    let mut metadata = c.benchmark_group("pipeline-metadata-v1");
    for leaves in [1000, 10_000] {
        let bytes = manifest_bytes(leaves);
        assert_eq!(Manifest::parse(&bytes).unwrap().leaves.len(), leaves);
        metadata.bench_with_input(BenchmarkId::from_parameter(leaves), &bytes, |b, bytes| {
            b.iter(|| black_box(Manifest::parse(black_box(bytes)).unwrap()));
        });
    }
    metadata.finish();

    let mut selection = c.benchmark_group("pipeline-lod-selection-v1");
    for leaves in [1000, 10_000] {
        let mut replay = SelectionReplay::new(leaves);
        for _ in 0..8 {
            let ranges = replay.step();
            replay.validate(&ranges);
        }
        selection.bench_function(BenchmarkId::from_parameter(leaves), |b| {
            b.iter(|| {
                for _ in 0..8 {
                    black_box(replay.step());
                }
            })
        });
    }
    selection.finish();

    let mut pressure = PressureReplay::new();
    for _ in 0..6 {
        let (files, bridge) = pressure.step();
        pressure.validate(&files, &bridge);
    }
    c.bench_function("pipeline-relocation-v1/pressure", |b| {
        b.iter(|| {
            for _ in 0..6 {
                black_box(pressure.step());
            }
        })
    });

    let mut cache = CacheReplay::new(1024);
    assert_eq!(cache.admit(), (true, 2));
    assert!(cache.pinned_is_ready());
    c.bench_function("pipeline-cache-admission-v1/1024", |b| {
        b.iter_batched(
            || CacheReplay::new(1024),
            |mut replay| black_box(replay.admit()),
            BatchSize::SmallInput,
        );
    });

    let mut resident = ResidentReplay::new(1000);
    for _ in 0..8 {
        assert!(resident.step() > 0);
    }
    c.bench_function("pipeline-scheduler-v1/resident", |b| {
        b.iter(|| {
            for _ in 0..8 {
                black_box(resident.step());
            }
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = loading::workloads, policies
}
criterion_main!(benches);
