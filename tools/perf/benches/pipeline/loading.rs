use criterion::{BenchmarkId, Criterion, Throughput};
use dimensify_perf::{
    SogFixture,
    pipeline::{binary_ply, composition_ranges, decode_packed, sog_bundle},
};
use dimensify_splat::{
    DecodedSplat, decode_splat,
    renderer::{DecodedChunk, compose::compose, source::SourceData},
};
use std::{hint::black_box, sync::Arc};

/// Check the authored rows rather than decoder alignment padding before timing.
fn validate(decoded: &DecodedSplat, count: usize) {
    assert_eq!(decoded.count, count);
    let cloud = &decoded.cloud;
    assert!(cloud.position_visibility.len() >= count);
    assert_eq!(
        cloud.position_visibility.len(),
        cloud.spherical_harmonic.len()
    );
    assert_eq!(cloud.position_visibility.len(), cloud.rotation.len());
    assert_eq!(cloud.position_visibility.len(), cloud.scale_opacity.len());
    assert!(decoded.bounds.0.is_finite() && decoded.bounds.1.is_finite());
    assert!(decoded.bounds.0.cmple(decoded.bounds.1).all());
    assert!(decoded.bounds.0.cmplt(decoded.bounds.1).any());
    for index in 0..count {
        let point = bevy_math::Vec3::from_array(cloud.position_visibility[index].position);
        assert!(point.is_finite());
        assert!(point.cmpge(decoded.bounds.0).all() && point.cmple(decoded.bounds.1).all());
        let scale = cloud.scale_opacity[index];
        assert!(scale.scale.iter().all(|v| v.is_finite() && *v > 0.));
        assert!((0. ..=1.).contains(&scale.opacity));
        let norm = cloud.rotation[index]
            .rotation
            .iter()
            .map(|v| v * v)
            .sum::<f32>();
        assert!((norm - 1.).abs() < 1e-5);
        assert!(
            cloud.spherical_harmonic[index]
                .coefficients
                .iter()
                .all(|v| v.is_finite())
        );
    }
    assert!(
        cloud.spherical_harmonic[0].coefficients[3..]
            .iter()
            .any(|v| *v != 0.)
    );
}

pub fn workloads(criterion: &mut Criterion) {
    let mut ply = criterion.benchmark_group("pipeline-ply-v1");
    for count in [10_000, 100_000] {
        let bytes = binary_ply(count);
        let decoded = decode_splat("fixture.ply", &bytes).unwrap();
        validate(&decoded, count);
        drop(decoded);
        ply.throughput(Throughput::Elements(count as u64));
        ply.bench_with_input(BenchmarkId::from_parameter(count), &bytes, |b, bytes| {
            b.iter(|| black_box(decode_splat("fixture.ply", black_box(bytes)).unwrap()));
        });
    }
    ply.finish();

    let fixture = SogFixture::new();
    let bytes = sog_bundle(&fixture);
    let decoded = decode_splat("fixture.sog", &bytes).unwrap();
    validate(&decoded, 65_536);
    let mut bundle = criterion.benchmark_group("pipeline-sog-bundle-v1");
    bundle.throughput(Throughput::Elements(65_536));
    bundle.bench_function("65536", |b| {
        b.iter(|| black_box(decode_splat("fixture.sog", black_box(&bytes)).unwrap()));
    });
    bundle.finish();

    let packed = decode_packed(&fixture);
    assert_eq!(packed.count(), 65_536);
    assert_eq!(packed.means_lo().dimensions(), [256, 256]);
    assert_eq!(packed.texture_bytes(), 6 * 256 * 256 * 4 + 960 * 4);
    assert_eq!(packed.higher_order().unwrap().bands(), 3);
    assert_eq!(packed.higher_order().unwrap().palette_count(), 64);
    let packed = SourceData::Packed(Arc::new(packed));
    assert!(packed.expanded().is_err());
    assert!(packed.retained_bytes() < 65_536 * DecodedChunk::bytes_per_splat() as u64);
    drop(packed);
    let mut packed = criterion.benchmark_group("pipeline-sog-packed-v1");
    packed.throughput(Throughput::Elements(65_536));
    packed.bench_function("65536", |b| {
        b.iter(|| black_box(decode_packed(black_box(&fixture))));
    });
    packed.finish();

    let chunk = DecodedChunk {
        cloud: decoded.cloud.into(),
        count: decoded.count,
    };
    let ranges = composition_ranges();
    let output = compose(&ranges, |_| Some(&chunk)).unwrap();
    assert_eq!(
        ranges.iter().map(|range| range.count).sum::<usize>(),
        65_536
    );
    assert_eq!(output.position_visibility.len(), 65_536);
    let source = chunk.cloud.expanded().unwrap();
    for (index, range) in ranges.iter().enumerate() {
        let output_index = index * 1024;
        assert_eq!(
            output.position_visibility[output_index].position,
            source.position_visibility[range.offset].position
        );
        assert_eq!(
            output.spherical_harmonic[output_index].coefficients,
            source.spherical_harmonic[range.offset].coefficients
        );
        assert_eq!(
            output.rotation[output_index].rotation,
            source.rotation[range.offset].rotation
        );
        assert_eq!(
            output.scale_opacity[output_index].scale,
            source.scale_opacity[range.offset].scale
        );
        assert_eq!(
            output.scale_opacity[output_index].opacity,
            source.scale_opacity[range.offset].opacity
        );
    }
    drop(output);
    let mut composition = criterion.benchmark_group("pipeline-compose-v1");
    composition.throughput(Throughput::Elements(65_536));
    composition.bench_function("65536", |b| {
        b.iter(|| black_box(compose(black_box(&ranges), |_| Some(black_box(&chunk))).unwrap()));
    });
    composition.finish();
}
