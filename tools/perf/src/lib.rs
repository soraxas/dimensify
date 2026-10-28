//! Deterministic CPU benchmark inputs; never linked by the viewer.
use bevy_math::{Mat4, Vec2, Vec3};
use dimensify_splat::streaming::{Bounds, Leaf, Manifest, PixelView, SplatRange};
use sog_codec::expanded::{Expanded, SogMeta};
use std::collections::HashMap;

pub fn positions(count: usize) -> Vec<f32> {
    let mut seed = 0x1234_5678_u32;
    (0..count * 3)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / 16_777_216. - 0.5) * 1024.
        })
        .collect()
}

pub fn lod_manifest(leaves: usize) -> Manifest {
    let bound = Bounds {
        min: Vec3::new(-1., -1., -6.),
        max: Vec3::new(1., 1., -4.),
    };
    let counts = [10_000, 2_500, 625];
    Manifest {
        bound,
        filenames: ["fine", "middle", "coarse"]
            .map(|level| format!("{level}/meta.json"))
            .into(),
        environment: None,
        file_counts: counts.map(|count| count * leaves).into(),
        leaves: (0..leaves)
            .map(|i| Leaf {
                bound,
                lods: counts
                    .iter()
                    .enumerate()
                    .map(|(level, &count)| {
                        (
                            level,
                            SplatRange {
                                file: level,
                                offset: i * count,
                                count,
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn camera_path() -> Vec<PixelView> {
    [2., 5., 20., 5., 2.]
        .map(|span| PixelView {
            clip_from_local: Mat4::orthographic_rh(
                -span / 2.,
                span / 2.,
                -span / 2.,
                span / 2.,
                0.1,
                1000.,
            ),
            viewport: Vec2::splat(200.),
        })
        .into()
}

pub struct SogFixture {
    pub meta: SogMeta,
    pub files: HashMap<String, Vec<u8>>,
}

impl SogFixture {
    pub fn new() -> Self {
        let metadata = serde_json::json!({
            "version": 2, "count": 65_536,
            "means": {"mins": [-1., -1., -1.], "maxs": [1., 1., 1.],
                      "files": ["means_l.webp", "means_u.webp"]},
            "quats": {"files": ["quats.webp"]},
            "scales": {"codebook": [-2., -1., 0., 1.], "files": ["scales.webp"]},
            "sh0": {"codebook": [-1., 0., 1., 2.], "files": ["sh0.webp"]},
            "shN": {"count": 64, "bands": 3, "codebook": [-1., 0., 1., 2.],
                    "files": ["centroids.webp", "labels.webp"]}
        });
        let meta =
            sog_codec::expanded::parse_meta(&serde_json::to_vec(&metadata).unwrap()).unwrap();
        let mut files = HashMap::new();
        for (name, kind) in [
            ("means_l.webp", 0),
            ("means_u.webp", 1),
            ("quats.webp", 2),
            ("scales.webp", 3),
            ("sh0.webp", 3),
            ("labels.webp", 4),
        ] {
            let rgba: Vec<u8> = (0..65_536_u32)
                .flat_map(|i| match kind {
                    0 => [i as u8, (i >> 3) as u8, (i >> 5) as u8, 255],
                    1 => [(i >> 8) as u8, (i >> 7) as u8, (i >> 6) as u8, 255],
                    2 => [128, 128, 128, 252 + (i % 4) as u8],
                    3 => [(i % 4) as u8, (i % 4) as u8, (i % 4) as u8, 255],
                    _ => [(i % 64) as u8, 0, 0, 255],
                })
                .collect();
            files.insert(name.into(), webp(&rgba, 256, 256));
        }
        let centroids: Vec<u8> = (0..960).flat_map(|i| [(i % 4) as u8, 1, 2, 255]).collect();
        files.insert("centroids.webp".into(), webp(&centroids, 960, 1));
        Self { meta, files }
    }
    pub fn decode(&self) -> Expanded {
        sog_codec::expanded::decode_expanded(&self.meta, &self.files, 16).unwrap()
    }
}

impl Default for SogFixture {
    fn default() -> Self {
        Self::new()
    }
}

fn webp(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut encoded = vec![];
    image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
        .encode(rgba, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use dimensify_splat::streaming::targets;
    use splat_sort::DepthSorter;

    #[test]
    fn seeded_positions_match_reference_sort_at_capture_scale() {
        let p = positions(100_000);
        assert_eq!(p.len(), 300_000);
        assert_eq!(p, positions(100_000));
        let mut sorter = DepthSorter::new(p.clone()).unwrap();
        for axis in [[0., 0., 1., 0.], [0.3, -0.5, -0.8, 6.]] {
            let depth = |i: u32| {
                let p = &p[i as usize * 3..];
                p[0] * axis[0] + p[1] * axis[1] + p[2] * axis[2] + axis[3]
            };
            let mut expected: Vec<u32> = (0..100_000).collect();
            expected.sort_by(|a, b| depth(*a).total_cmp(&depth(*b)));
            assert_eq!(sorter.sort(&axis).unwrap(), expected);
        }
    }

    #[test]
    fn lod_camera_path_refines_and_coarsens_every_leaf() {
        let manifest = lod_manifest(1000);
        assert_eq!(manifest.leaves.len(), 1000);
        let mut previous = vec![];
        let path = camera_path();
        let mut changed = false;
        for view in path {
            let before = previous.clone();
            targets(&manifest, view, 2., &mut previous);
            assert_eq!(previous.len(), 1000);
            assert!(previous.iter().all(|i| *i < 3));
            changed |= before != previous && !before.is_empty();
        }
        assert!(changed, "camera path must exercise LOD transitions");
    }

    #[test]
    fn sog_fixture_decodes_full_attribute_arrays_with_known_values() {
        let fixture = SogFixture::new();
        assert_eq!(fixture.meta.count, 65_536);
        let decoded = fixture.decode();
        let padded = 65_568;
        assert_eq!(decoded.positions.len(), padded * 4);
        assert_eq!(decoded.rotations.len(), padded * 4);
        assert_eq!(decoded.scales.len(), padded * 4);
        assert_eq!(decoded.harmonics.len(), padded * 48);
        assert!(decoded.positions.iter().all(|v| v.is_finite()));
        assert_eq!(decoded.positions[3], 1.);
        assert!((decoded.scales[0] - (-2_f32).exp()).abs() < 1e-6);
        assert_eq!(decoded.scales[3], 1.);
        assert_eq!(decoded.harmonics[0], -1.);
        let rotation = decoded.rotations[..4].iter().map(|v| v * v).sum::<f32>();
        assert!((rotation - 1.).abs() < 1e-6);
        assert!(decoded.harmonics[3..48].iter().any(|v| *v != 0.));
        assert_eq!(fixture.files, SogFixture::new().files);
    }
}
