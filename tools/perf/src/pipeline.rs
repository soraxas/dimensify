//! Deterministic inputs for production loading and composition benchmarks.
use crate::SogFixture;
use bevy_gaussian_splatting::io::sog_packed::{self, DecodeLimits, PackedSog};
use dimensify_splat::streaming::SplatRange;
use std::io::{Cursor, Write};

/// Generate standard binary little-endian Gaussian PLY with all SH3 properties.
pub fn binary_ply(count: usize) -> Vec<u8> {
    assert!(count > 0);
    let mut header = format!("ply\nformat binary_little_endian 1.0\nelement vertex {count}\n");
    for name in [
        "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "scale_0", "scale_1", "scale_2", "opacity",
        "rot_0", "rot_1", "rot_2", "rot_3",
    ] {
        header.push_str(&format!("property float {name}\n"));
    }
    for index in 0..45 {
        header.push_str(&format!("property float f_rest_{index}\n"));
    }
    header.push_str("end_header\n");
    let mut bytes = header.into_bytes();
    bytes.reserve(count * 59 * 4);
    for index in 0..count {
        let position = [
            (index % 100) as f32 / 10.,
            (index / 100 % 100) as f32 / 10.,
            -5. + (index / 10_000) as f32 / 10.,
        ];
        for value in position
            .into_iter()
            .chain([0.25_f32, 0.5, 0.75, -2., -1., 0., 0., 2., 0., 0., 0.])
            .chain((1..=45).map(|i| i as f32 / 1000.))
        {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

/// Archive the existing deterministic fixture as a real SOG v2 ZIP bundle.
pub fn sog_bundle(fixture: &SogFixture) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    writer.start_file("meta.json", options).unwrap();
    writer
        .write_all(&serde_json::to_vec(&fixture.meta).unwrap())
        .unwrap();
    // A stable entry order keeps generated archives byte-for-byte reproducible.
    let mut names: Vec<_> = fixture.files.keys().collect();
    names.sort_unstable();
    for name in names {
        writer.start_file(name.as_str(), options).unwrap();
        writer.write_all(&fixture.files[name]).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Invoke production packed validation and WebP decode without float expansion.
pub fn decode_packed(fixture: &SogFixture) -> PackedSog {
    sog_packed::decode(
        &fixture.meta,
        &fixture.files,
        3,
        DecodeLimits {
            max_texture_dimension: 8192,
            max_texture_bytes: dimensify_splat::renderer::source::packed_reservation(
                fixture.meta.count,
            )
            .saturating_sub(8192) as u64,
        },
    )
    .unwrap()
}

/// Reverse the order of 64 disjoint ranges to exercise non-contiguous copying.
pub fn composition_ranges() -> Vec<SplatRange> {
    (0..64)
        .rev()
        .map(|index| SplatRange {
            file: 0,
            offset: index * 1024,
            count: 1024,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_math::Vec3;
    use dimensify_splat::decode_splat;
    use dimensify_splat::renderer::{DecodedChunk, compose::compose};

    #[test]
    fn binary_ply_decodes_authored_attributes_and_bounds_at_both_scales() {
        for count in [10_000, 100_000] {
            let bytes = binary_ply(count);
            assert_eq!(bytes, binary_ply(count));
            let decoded = decode_splat("fixture.ply", &bytes).unwrap();
            assert_eq!(decoded.count, count);
            assert_eq!(
                decoded.cloud.position_visibility.len(),
                count.next_multiple_of(32)
            );
            assert_eq!(decoded.bounds.0, Vec3::new(0., 0., -5.));
            assert_eq!(
                decoded.bounds.1,
                Vec3::new(9.9, 9.9, -5. + (count / 10_000 - 1) as f32 / 10.)
            );
            assert_eq!(
                decoded.cloud.position_visibility[101].position,
                [0.1, 0.1, -5.]
            );
            assert_eq!(decoded.cloud.spherical_harmonic[0].coefficients[0], 0.25);
            // Graphdeco channel-major f_rest_1 becomes SH coefficient 2, red.
            assert_eq!(decoded.cloud.spherical_harmonic[0].coefficients[6], 0.002);
            assert_eq!(decoded.cloud.spherical_harmonic[0].coefficients[4], 0.016);
            assert_eq!(
                decoded.cloud.rotation[0]
                    .rotation
                    .iter()
                    .map(|v| v * v)
                    .sum::<f32>(),
                1.
            );
            assert!((decoded.cloud.scale_opacity[0].scale[0] - (-2_f32).exp()).abs() < 1e-6);
            assert_eq!(decoded.cloud.scale_opacity[0].opacity, 0.5);
        }
    }

    #[test]
    fn sog_archive_matches_expansion_and_retains_packed_sh3_textures() {
        let fixture = SogFixture::new();
        let bytes = sog_bundle(&fixture);
        assert_eq!(bytes, sog_bundle(&fixture));
        let decoded = decode_splat("fixture.sog", &bytes).unwrap();
        assert_eq!(decoded.count, 65_536);
        assert_eq!(decoded.cloud.position_visibility.len(), 65_568);
        let reference = fixture.decode();
        for i in [0, 1, 32768, 65535] {
            assert_eq!(
                decoded.cloud.position_visibility[i].position,
                reference.positions[i * 4..i * 4 + 3]
            );
            assert_eq!(
                decoded.cloud.spherical_harmonic[i].coefficients,
                reference.harmonics[i * 48..(i + 1) * 48]
            );
        }
        assert!(decoded.bounds.0.cmplt(decoded.bounds.1).all());
        assert!(decoded.bounds.0.cmpge(Vec3::splat(-1.719)).all());
        assert!(decoded.bounds.1.cmple(Vec3::splat(1.719)).all());
        let packed = decode_packed(&fixture);
        assert_eq!(packed.count(), 65_536);
        assert_eq!(packed.means_lo().dimensions(), [256, 256]);
        assert_eq!(packed.means_lo().texels()[..4], [0, 0, 0, 255]);
        assert_eq!(packed.texture_bytes(), 6 * 256 * 256 * 4 + 960 * 4);
        let higher = packed.higher_order().unwrap();
        assert_eq!(higher.bands(), 3);
        assert_eq!(higher.palette_count(), 64);
        assert_eq!(higher.centroids().dimensions(), [960, 1]);
        let retained =
            dimensify_splat::renderer::source::SourceData::Packed(std::sync::Arc::new(packed));
        assert!(retained.expanded().is_err());
        assert!(retained.retained_bytes() < 65_536 * DecodedChunk::bytes_per_splat() as u64);
    }

    #[test]
    fn composition_reorders_all_ranges_without_source_padding() {
        let fixture = SogFixture::new();
        let decoded = decode_splat("fixture.sog", &sog_bundle(&fixture)).unwrap();
        let chunk = DecodedChunk {
            cloud: decoded.cloud.into(),
            count: decoded.count,
        };
        let ranges = composition_ranges();
        assert_eq!(ranges.len(), 64);
        assert_eq!(ranges.iter().map(|r| r.count).sum::<usize>(), 65_536);
        let mut offsets: Vec<_> = ranges.iter().map(|r| r.offset).collect();
        offsets.sort_unstable();
        assert_eq!(offsets, (0..64).map(|i| i * 1024).collect::<Vec<_>>());
        let output = compose(&ranges, |_| Some(&chunk)).unwrap();
        assert_eq!(output.position_visibility.len(), 65_536);
        let source = chunk.cloud.expanded().unwrap();
        for (index, range) in ranges.iter().enumerate() {
            for offset in [0, 1023] {
                let actual = index * 1024 + offset;
                let expected = range.offset + offset;
                assert_eq!(
                    output.position_visibility[actual].position,
                    source.position_visibility[expected].position
                );
                assert_eq!(
                    output.spherical_harmonic[actual].coefficients,
                    source.spherical_harmonic[expected].coefficients
                );
                assert_eq!(
                    output.rotation[actual].rotation,
                    source.rotation[expected].rotation
                );
                assert_eq!(
                    output.scale_opacity[actual].scale,
                    source.scale_opacity[expected].scale
                );
            }
        }
    }
}
