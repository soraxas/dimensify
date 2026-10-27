//! Filesystem-only producer-bounds diagnostic. No network, window or renderer.

use super::manifest::{Bounds, Manifest, SplatRange};
use bevy::prelude::*;
use std::collections::BTreeMap;

/// Axis-aligned radii of the decoded one-sigma ellipsoid, in source space.
/// Input rotation is SOG's WXYZ, not glam's XYZW. Do not renormalize it:
/// compare what the shader actually renders after quantized decoding.
fn support_radius(rotation: [f32; 4], scale: [f32; 3]) -> [f64; 3] {
    let [w, x, y, z] = rotation.map(f64::from);
    let scale = scale.map(f64::from);
    // Rows of the conventional rotation matrix. The WGSL helper constructs
    // its transpose, then computes transpose(S * R) * (S * R): same diagonal.
    let rows = [
        [
            1. - 2. * (y * y + z * z),
            2. * (x * y - w * z),
            2. * (x * z + w * y),
        ],
        [
            2. * (x * y + w * z),
            1. - 2. * (x * x + z * z),
            2. * (y * z - w * x),
        ],
        [
            2. * (x * z - w * y),
            2. * (y * z + w * x),
            1. - 2. * (x * x + y * y),
        ],
    ];
    rows.map(|row| {
        row.into_iter()
            .zip(scale)
            .map(|(r, s)| (r * s).powi(2))
            .sum::<f64>()
            .sqrt()
    })
}

fn overshoot(bound: Bounds, position: [f32; 3], radius: [f64; 3]) -> f64 {
    (0..3).fold(0f64, |maximum, axis| {
        let p = f64::from(position[axis]);
        maximum
            .max(f64::from(bound.min[axis]) - (p - radius[axis]))
            .max(p + radius[axis] - f64::from(bound.max[axis]))
    })
}

struct AuthoredRange {
    leaf: usize,
    level: usize,
    bound: Bounds,
    range: SplatRange,
}

fn authored_ranges(node: &serde_json::Value, leaf: &mut usize, ranges: &mut Vec<AuthoredRange>) {
    if let Some(children) = node["children"].as_array() {
        for child in children {
            authored_ranges(child, leaf, ranges);
        }
    } else {
        let bound: Bounds =
            serde_json::from_value(node["bound"].clone()).expect("validated bounds");
        let lods: BTreeMap<usize, SplatRange> =
            serde_json::from_value(node["lods"].clone()).expect("validated ranges");
        ranges.extend(lods.into_iter().map(|(level, range)| AuthoredRange {
            leaf: *leaf,
            level,
            bound,
            range,
        }));
        *leaf += 1;
    }
}

#[derive(Default)]
struct AuditTotals {
    splats: usize,
    ranges: usize,
    outside: [usize; 3],
    ranges_outside: [usize; 3],
    maximum: [f64; 3],
    worst: [String; 3],
}

#[test]
fn authored_bounds_audit_keeps_dominated_lods() {
    let bytes = br#"{
        "version":1,"lodLevels":2,"filenames":["fine/meta.json","coarse/meta.json"],
        "tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},
        "lods":{"0":{"file":0,"offset":0,"count":1},"1":{"file":1,"offset":0,"count":2}}}
    }"#;
    let manifest = Manifest::parse(bytes).expect("valid hierarchy");
    assert_eq!(
        manifest.leaves[0].lods.len(),
        1,
        "production discards dominated coarse range"
    );
    let raw: serde_json::Value = serde_json::from_slice(bytes).expect("JSON");
    let mut ranges = Vec::new();
    let mut leaves = 0;
    authored_ranges(&raw["tree"], &mut leaves, &mut ranges);
    assert_eq!(leaves, 1);
    assert_eq!(ranges.len(), 2, "audit must cover all source data");
    assert_eq!(ranges[1].range.count, 2);
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR; reports authored bounds versus all decoded LODs"]
fn complete_local_fixture_audits_decoded_gaussian_support() {
    use bevy_gaussian_splatting::io::sog::{decode_sog, parse_meta};

    let root =
        std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").expect("fixture directory"));
    let bytes = std::fs::read(root.join("lod-meta.json")).expect("manifest bytes");
    let manifest = Manifest::parse(&bytes).expect("production hierarchy validation");
    let raw: serde_json::Value = serde_json::from_slice(&bytes).expect("manifest JSON");
    let mut ranges = Vec::new();
    let mut leaves = 0;
    authored_ranges(&raw["tree"], &mut leaves, &mut ranges);
    assert_eq!(leaves, manifest.leaves.len());
    let mut levels: BTreeMap<usize, AuditTotals> = BTreeMap::new();
    let mut total = 0;
    // The environment has no leaf range: it is deliberately excluded rather
    // than incorrectly tested against the hierarchy's source-space bound.
    for (file, path) in manifest.filenames.iter().enumerate() {
        let meta_path = root.join(path);
        let meta = parse_meta(&std::fs::read(&meta_path).expect(path)).expect(path);
        assert_eq!(meta.count, manifest.file_counts[file]);
        let files = meta
            .referenced_files()
            .into_iter()
            .map(|name| {
                let payload = meta_path.parent().expect("chunk directory").join(name);
                (name.to_string(), std::fs::read(payload).expect(name))
            })
            .collect();
        let cloud = decode_sog(&meta, &files).expect(path);
        let mut visited = 0;
        for authored in ranges.iter().filter(|r| r.range.file == file) {
            let stats = levels.entry(authored.level).or_default();
            stats.ranges += 1;
            let mut range_outside = [false; 3];
            // Generous allowance for seven-significant-digit manifest bounds
            // and f32 math, not an allowance for arbitrary codec quantization.
            let tolerance = 1e-5
                * f64::from(
                    authored
                        .bound
                        .min
                        .abs()
                        .max(authored.bound.max.abs())
                        .max_element()
                        .max(1.),
                );
            let end = authored.range.offset + authored.range.count;
            for i in authored.range.offset..end {
                let p = cloud.position_visibility[i].position;
                let attr = &cloud.scale_opacity[i];
                let radius = support_radius(cloud.rotation[i].rotation, attr.scale);
                assert!(p.iter().all(|v| v.is_finite()));
                assert!(radius.iter().all(|v| v.is_finite() && *v >= 0.));
                for (support, sigma) in [0., 1., 3.].into_iter().enumerate() {
                    let excess = overshoot(authored.bound, p, radius.map(|r| r * sigma));
                    if excess > tolerance {
                        stats.outside[support] += 1;
                        range_outside[support] = true;
                    }
                    if excess > stats.maximum[support] {
                        stats.maximum[support] = excess;
                        stats.worst[support] = format!(
                            "leaf={} file={path} row={i} position={p:?} scale={:?} opacity={} tolerance={tolerance:.8}",
                            authored.leaf, attr.scale, attr.opacity
                        );
                    }
                }
                stats.splats += 1;
                visited += 1;
            }
            for (count, outside) in stats.ranges_outside.iter_mut().zip(range_outside) {
                *count += usize::from(outside);
            }
        }
        assert_eq!(
            visited, meta.count,
            "audit must visit each logical row in {path}"
        );
        total += visited;
        eprintln!("bounds audited {path}: {visited} splats");
        // Drop compressed images and decoded planar attributes per file.
    }
    assert!(total > 0 && leaves > 0);
    eprintln!(
        "BOUNDS AUDIT: {leaves} leaves, {} chunks, {total} splats; environment excluded",
        manifest.filenames.len()
    );
    for (level, stats) in levels {
        eprintln!(
            "LOD {level}: {} splats, {} ranges; centre/1sigma/3sigma outside={:?}, ranges={:?}, max_excess={:?}",
            stats.splats, stats.ranges, stats.outside, stats.ranges_outside, stats.maximum
        );
        for (support, worst) in stats.worst.into_iter().enumerate() {
            eprintln!("  support {}sigma worst: {worst}", [0, 1, 3][support]);
        }
    }
    // Deliberately no containment assertion: the published format does not
    // specify a sigma cutoff, and older producers may bound pre-quantized data.
    // Passing means the diagnostic completed, NOT that the dataset is valid.
}

#[test]
fn ellipsoid_support_uses_wxyz_and_rotated_anisotropic_scales() {
    assert_eq!(support_radius([1., 0., 0., 0.], [2., 3., 4.]), [2., 3., 4.]);
    let q = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let actual = support_radius([q.w, q.x, q.y, q.z], [2., 3., 4.]);
    for (actual, expected) in actual.into_iter().zip([3., 2., 4.]) {
        assert!((actual - expected).abs() < 1e-5);
    }
    let q = Quat::from_rotation_z(std::f32::consts::FRAC_PI_4);
    let actual = support_radius([q.w, q.x, q.y, q.z], [2., 4., 1.]);
    for (actual, expected) in actual.into_iter().zip([10f64.sqrt(), 10f64.sqrt(), 1.]) {
        assert!((actual - expected).abs() < 1e-5);
    }
}

#[test]
fn bounds_probe_distinguishes_centres_from_gaussian_support() {
    let bounds = Bounds {
        min: Vec3::splat(-1.),
        max: Vec3::splat(1.),
    };
    assert_eq!(overshoot(bounds, [0.9, 0., 0.], [0.; 3]), 0.);
    assert!((overshoot(bounds, [0.9, 0., 0.], [0.2; 3]) - 0.1).abs() < 1e-6);
    assert!((overshoot(bounds, [-0.9, 0., 0.], [0.2; 3]) - 0.1).abs() < 1e-6);
    assert_eq!(overshoot(bounds, [0.; 3], [1.; 3]), 0.);
}
