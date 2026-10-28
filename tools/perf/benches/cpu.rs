use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use dimensify_perf::{SogFixture, camera_path, lod_manifest, positions};
use dimensify_splat::streaming::targets;
use splat_sort::DepthSorter;
use std::{hint::black_box, time::Duration};

fn workloads(criterion: &mut Criterion) {
    let fixture = SogFixture::new();
    let decoded = fixture.decode();
    assert_eq!(fixture.meta.count, 65_536);
    assert_eq!(decoded.positions.len(), 65_568 * 4);
    assert_eq!(decoded.harmonics.len(), 65_568 * 48);
    assert!(
        decoded.scales[..65_536 * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|v| v[..3].iter().all(|x| *x > 0.))
    );
    drop(decoded);
    let mut decode = criterion.benchmark_group("sog-decode-v1");
    decode.throughput(Throughput::Elements(65_536));
    decode.bench_function("65536", |b| {
        b.iter(|| {
            // Deliberate slowdown to verify the hosted regression gate.
            for _ in 0..3 {
                black_box(black_box(&fixture).decode());
            }
        })
    });
    decode.finish();

    let mut sort = criterion.benchmark_group("depth-sort-v1");
    for count in [100_000, 1_000_000] {
        let p = positions(count);
        let axes = [[0., 0., 1., 0.], [0.3, -0.5, -0.8, 6.]];
        let mut sorter = DepthSorter::new(p.clone()).unwrap();
        for axis in axes {
            let depth = |i: u32| {
                let p = &p[i as usize * 3..];
                p[0] * axis[0] + p[1] * axis[1] + p[2] * axis[2] + axis[3]
            };
            let mut expected: Vec<u32> = (0..count as u32).collect();
            expected.sort_by(|a, b| depth(*a).total_cmp(&depth(*b)));
            assert_eq!(sorter.sort(&axis).unwrap(), expected);
        }
        let mut frame = 0;
        sort.throughput(Throughput::Elements(count as u64));
        sort.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                frame = (frame + 1) % axes.len();
                black_box(sorter.sort(black_box(&axes[frame])).unwrap())
            });
        });
    }
    sort.finish();

    let path = camera_path();
    let mut lod = criterion.benchmark_group("lod-targets-v1");
    for count in [1000, 10_000] {
        let manifest = lod_manifest(count);
        let mut previous = vec![];
        for (view, expected) in path.iter().zip([0, 1, 2, 1, 0]) {
            targets(&manifest, *view, 2., &mut previous);
            assert_eq!(previous.len(), count);
            assert!(previous.iter().all(|level| *level == expected));
        }
        lod.throughput(Throughput::Elements((count * path.len()) as u64));
        lod.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, _| {
            b.iter(|| {
                for view in &path {
                    targets(
                        black_box(&manifest),
                        black_box(*view),
                        2.,
                        black_box(&mut previous),
                    );
                }
                black_box(&previous);
            });
        });
    }
    lod.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = workloads
}
criterion_main!(benches);
