//! Opt-in harness investigation. These workloads do not replace the v1 CI gate.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use dimensify_splat::streaming::runtime::test_support::benchmarks::CacheReplay;
use std::{hint::black_box, time::Duration};

fn admission(c: &mut Criterion) {
    let mut replay = CacheReplay::new(1024);
    assert_eq!(replay.admit(), (true, 2));
    assert!(replay.pinned_is_ready());

    // Reproduce the v1 gate: consuming the input also times its destruction.
    c.bench_function("diagnostic-cache-v1/legacy-small-input", |b| {
        b.iter_batched(
            || CacheReplay::new(1024),
            |mut replay| black_box(replay.admit()),
            BatchSize::SmallInput,
        );
    });
    c.bench_function("diagnostic-cache-v1/admit-only-small-input", |b| {
        b.iter_batched_ref(
            || CacheReplay::new(1024),
            |replay| black_box(replay.admit()),
            BatchSize::SmallInput,
        );
    });
    // Borrowing keeps input destruction outside the timed routine; one input
    // per iteration also avoids a sample-dependent population of live entries.
    c.bench_function("diagnostic-cache-v1/admit-only-per-iteration", |b| {
        b.iter_batched_ref(
            || CacheReplay::new(1024),
            |replay| black_box(replay.admit()),
            BatchSize::PerIteration,
        );
    });
    // Keep the input population bounded and independent of Criterion's sample
    // iteration count, while amortizing the timer over eight admissions.
    // Whole-input destruction remains outside the timed routine.
    c.bench_function("cache-admission-v2/1024", |b| {
        b.iter_batched_ref(
            || CacheReplay::new(1024),
            |replay| black_box(replay.admit()),
            BatchSize::NumIterations(8),
        );
    });
    c.bench_function("cache-teardown-v2/1024", |b| {
        b.iter_batched(
            || CacheReplay::new(1024),
            |replay| drop(black_box(replay)),
            BatchSize::PerIteration,
        );
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(20);
    targets = admission
}
criterion_main!(benches);
