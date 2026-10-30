//! SOG v2 decoder ("Spatially Ordered Gaussians", PlayCanvas `splat-transform`).
//!
//! SOG content is a `meta.json` describing splat count and per-attribute
//! encodings, plus sibling lossless WebP images packing one splat per pixel
//! (row-major):
//!
//! - `means_l.webp` / `means_u.webp`: 16-bit fixed-point positions (low/high
//!   bytes), lerped between `means.mins`/`means.maxs` of the log-transformed
//!   positions.
//! - `quats.webp`: smallest-three quaternions; alpha tags the largest
//!   component (252..=255 -> w/x/y/z).
//! - `scales.webp`: R/G/B index the 256-entry `scales.codebook` (log scales).
//! - `sh0.webp`: R/G/B index `sh0.codebook` (DC SH coefficients); alpha is
//!   sigmoid-encoded opacity.
//! - optional `shN_centroids.webp` + `shN_labels.webp`: palette of
//!   higher-band SH coefficient vectors with per-splat 16-bit labels.
//!
//! A bundled `.sog` file is a zip archive of the same contents.
//!
//! Decode math mirrors the reference reader in
//! `playcanvas/splat-transform/src/lib/readers/read-sog.ts`.

use std::collections::HashMap;

#[cfg(test)]
use crate::material::spherical_harmonics::SH_CHANNELS;

use crate::{
    gaussian::formats::planar_3d::PlanarGaussian3d,
    material::spherical_harmonics::SH_COEFF_COUNT_PER_CHANNEL,
};

pub use sog_codec::expanded::{
    SogCodebook, SogError, SogFiles, SogMeans, SogMeta, SogShN, parse_meta,
};

/// Decode with the engine-independent worker implementation.
pub fn decode_sog(
    meta: &SogMeta,
    files: &HashMap<String, Vec<u8>>,
) -> Result<PlanarGaussian3d, SogError> {
    let expanded = sog_codec::expanded::decode_expanded(meta, files, SH_COEFF_COUNT_PER_CHANNEL)?;
    Ok(from_expanded(expanded))
}

/// Transfer validated planar arrays without per-attribute decoding.
/// Compatible allocations are reused; unusual spare capacities retain a copy fallback.
pub fn from_expanded(expanded: sog_codec::expanded::Expanded) -> PlanarGaussian3d {
    let count = expanded.positions.len() / 4;
    PlanarGaussian3d {
        position_visibility: into_plane(expanded.positions, count),
        rotation: into_plane(expanded.rotations, count),
        scale_opacity: into_plane(expanded.scales, count),
        spherical_harmonic: into_plane(expanded.harmonics, count),
    }
}

fn into_plane<T: bytemuck::Pod>(values: Vec<f32>, count: usize) -> Vec<T> {
    let stride = std::mem::size_of::<T>() / std::mem::size_of::<f32>();
    assert_eq!(
        values.len(),
        count.checked_mul(stride).expect("plane length overflow")
    );
    // This checks allocation alignment AND capacity as well as the live length.
    // A slice cast alone cannot establish that Vec's allocation can be reused.
    match bytemuck::allocation::try_cast_vec(values) {
        Ok(plane) => plane,
        Err((_, values)) => {
            let mut plane = vec![T::zeroed(); count];
            bytemuck::cast_slice_mut(&mut plane).copy_from_slice(&values);
            plane
        }
    }
}

/// Decode a bundled `.sog` archive (zip of `meta.json` + WebP files).
/// Only metadata and textures consumed by the compiled SH degree are extracted.
/// This is a decoder, not an integrity check of unrelated archive contents.
pub fn decode_sog_bundle(bytes: &[u8]) -> Result<PlanarGaussian3d, SogError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;

    // Index names without decompressing or copying payloads. Preserve the
    // historical nested-basename lookup and last-entry-wins behavior.
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index)?;
        if entry.is_dir() {
            continue;
        }
        // key by basename: bundles may nest contents in a directory
        let name = entry
            .name()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        entries.insert(name, index);
    }

    let mut read = |name: &str| -> Result<Vec<u8>, SogError> {
        let index = entries
            .get(name)
            .ok_or_else(|| SogError::MissingFile(name.to_owned()))?;
        let mut entry = archive.by_index(*index)?;
        let mut contents = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut contents)
            .map_err(|source| SogError::ZipEntry {
                file: name.to_owned(),
                source,
            })?;
        Ok(contents)
    };

    let meta = parse_meta(&read("meta.json")?)?;
    let mut files = HashMap::new();
    for name in meta.required_files_for_decode(SH_COEFF_COUNT_PER_CHANNEL) {
        if !files.contains_key(name) {
            files.insert(name.to_owned(), read(name)?);
        }
    }
    decode_sog(&meta, &files)
}

#[cfg(test)]
#[path = "sog_packed_tests.rs"]
mod packed_tests;

#[cfg(test)]
#[path = "sog_gpu_tests.rs"]
mod gpu_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::spherical_harmonics::SH_COEFF_COUNT;

    /// Encode RGBA pixels as a lossless WebP.
    pub(super) fn webp(pixels: &[[u8; 4]], width: u32, height: u32) -> Vec<u8> {
        assert_eq!(pixels.len(), (width * height) as usize);
        let raw: Vec<u8> = pixels.iter().flatten().copied().collect();
        let mut buf = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(std::io::Cursor::new(&mut buf))
            .encode(&raw, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        buf
    }

    /// `logTransform(x) = sign(x) * ln(|x| + 1)` (the encoder side).
    fn log_transform(x: f64) -> f64 {
        x.signum() * (x.abs() + 1.0).ln()
    }

    /// Encode positions into the 16-bit lo/hi fixed-point scheme.
    fn encode_means(
        positions: &[[f64; 3]],
        mins: [f64; 3],
        maxs: [f64; 3],
        width: u32,
        height: u32,
    ) -> (Vec<u8>, Vec<u8>) {
        let mut lo = vec![[0u8; 4]; (width * height) as usize];
        let mut hi = vec![[0u8; 4]; (width * height) as usize];
        for (i, p) in positions.iter().enumerate() {
            for axis in 0..3 {
                let l = log_transform(p[axis]);
                let n = ((l - mins[axis]) / (maxs[axis] - mins[axis])).clamp(0.0, 1.0);
                let fixed = (n * 65535.0).round() as u16;
                lo[i][axis] = (fixed & 0xff) as u8;
                hi[i][axis] = (fixed >> 8) as u8;
            }
        }
        (webp(&lo, width, height), webp(&hi, width, height))
    }

    const MINS: [f64; 3] = [-3.0, -3.0, -3.0];
    const MAXS: [f64; 3] = [3.0, 3.0, 3.0];

    fn meta_json(count: usize, with_sh_n: bool) -> String {
        let scales_codebook: Vec<f32> = (0..256).map(|i| -5.0 + i as f32 * 0.01).collect();
        let sh0_codebook: Vec<f32> = (0..256).map(|i| -2.0 + i as f32 * 0.015).collect();
        let sh_n = if with_sh_n {
            format!(
                r#","shN":{{"count":1,"bands":1,"codebook":{},"files":["shN_centroids.webp","shN_labels.webp"]}}"#,
                serde_json::to_string(&(0..256).map(|i| i as f32 * 0.005).collect::<Vec<f32>>())
                    .unwrap()
            )
        } else {
            String::new()
        };
        format!(
            r#"{{
                "version": 2,
                "count": {count},
                "means": {{"mins": {mins:?}, "maxs": {maxs:?}, "files": ["means_l.webp", "means_u.webp"]}},
                "scales": {{"codebook": {scales}, "files": ["scales.webp"]}},
                "quats": {{"files": ["quats.webp"]}},
                "sh0": {{"codebook": {sh0}, "files": ["sh0.webp"]}}
                {sh_n}
            }}"#,
            mins = MINS,
            maxs = MAXS,
            scales = serde_json::to_string(&scales_codebook).unwrap(),
            sh0 = serde_json::to_string(&sh0_codebook).unwrap(),
        )
    }

    /// A 2x2-texture scene with `count` splats (texture holds 4).
    pub(super) fn fixture(count: usize, with_sh_n: bool) -> (SogMeta, HashMap<String, Vec<u8>>) {
        let positions = [
            [0.5, -1.25, 2.0],
            [-2.5, 0.0, 1.0],
            [1.5, 1.5, -1.5],
            [9.9, 9.9, 9.9], // trailing pixel beyond count=3
        ];
        let (lo, hi) = encode_means(&positions, MINS, MAXS, 2, 2);

        // quat (w largest): xyz bytes at 128 decode to tiny components, w ~ 1
        let quats = webp(&[[128, 128, 128, 252]; 4], 2, 2);
        let scales = webp(&[[10, 20, 30, 0]; 4], 2, 2);
        // sh0: RGB codebook indices, alpha = opacity
        let sh0 = webp(&[[0, 100, 255, 204]; 4], 2, 2);

        let mut files = HashMap::from([
            ("means_l.webp".to_string(), lo),
            ("means_u.webp".to_string(), hi),
            ("quats.webp".to_string(), quats),
            ("scales.webp".to_string(), scales),
            ("sh0.webp".to_string(), sh0),
        ]);

        if with_sh_n {
            // bands=1 -> 3 coeffs per palette entry, texture width 64*3
            let mut centroids = vec![[0u8; 4]; 64 * 3];
            centroids[0] = [10, 0, 0, 255];
            centroids[1] = [20, 0, 0, 255];
            centroids[2] = [30, 0, 0, 255];
            files.insert(
                "shN_centroids.webp".to_string(),
                webp(&centroids, 64 * 3, 1),
            );
            files.insert(
                "shN_labels.webp".to_string(),
                webp(&[[0, 0, 0, 255]; 4], 2, 2),
            );
        }

        let meta = parse_meta(meta_json(count, with_sh_n).as_bytes()).unwrap();
        (meta, files)
    }

    #[test]
    fn position_round_trip_within_quantization_tolerance() {
        let (meta, files) = fixture(3, false);
        let cloud = decode_sog(&meta, &files).unwrap();

        let expected = [[0.5, -1.25, 2.0], [-2.5, 0.0, 1.0], [1.5, 1.5, -1.5]];
        for (i, exp) in expected.iter().enumerate() {
            for axis in 0..3 {
                let got = cloud.position_visibility[i].position[axis];
                // 16-bit over a ~6-unit log range -> sub-millimeter precision
                assert!(
                    (got - exp[axis] as f32).abs() < 1e-3,
                    "splat {i} axis {axis}: got {got}, expected {}",
                    exp[axis]
                );
            }
        }
    }

    #[test]
    fn quats_decode_unit_norm_wxyz() {
        let (meta, files) = fixture(3, false);
        let cloud = decode_sog(&meta, &files).unwrap();

        for i in 0..3 {
            let q = cloud.rotation[i].rotation;
            let norm = q.iter().map(|c| c * c).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3, "splat {i} norm {norm}");
            // tag 252 marks w (index 0) as the largest component
            assert!(q[0] > 0.99, "splat {i} expected ~identity, got {q:?}");
        }
    }

    #[test]
    fn scales_and_sh0_resolve_through_codebooks() {
        let (meta, files) = fixture(3, false);
        let cloud = decode_sog(&meta, &files).unwrap();

        // scale codebook: -5.0 + idx * 0.01, exp applied directly
        let expected_scale = [
            (-5.0f32 + 0.10).exp(),
            (-5.0f32 + 0.20).exp(),
            (-5.0f32 + 0.30).exp(),
        ];
        for axis in 0..3 {
            let got = cloud.scale_opacity[0].scale[axis];
            assert!(
                (got - expected_scale[axis]).abs() < 1e-6,
                "axis {axis}: got {got}, expected {}",
                expected_scale[axis]
            );
        }

        // sh0 codebook: -2.0 + idx * 0.015 for indices [0, 100, 255]
        let expected_dc = [-2.0f32, -2.0 + 1.5, -2.0 + 3.825];
        for channel in 0..3 {
            let got = cloud.spherical_harmonic[0].coefficients[channel];
            assert!(
                (got - expected_dc[channel]).abs() < 1e-5,
                "channel {channel}: got {got}, expected {}",
                expected_dc[channel]
            );
        }

        // opacity: alpha 204 / 255
        let opacity = cloud.scale_opacity[0].opacity;
        assert!((opacity - 204.0 / 255.0).abs() < 1e-6, "opacity {opacity}");
    }

    #[test]
    fn authored_anisotropic_scales_are_not_mean_clamped() {
        let (mut meta, mut files) = fixture(1, false);
        meta.scales.codebook = vec![-12.0, 0.0, 0.0];
        files.insert("scales.webp".to_string(), webp(&[[0, 1, 2, 255]], 1, 1));

        let cloud = decode_sog(&meta, &files).unwrap();
        let expected = [0.000006144212f32, 1.0, 1.0];
        for (axis, expected) in expected.into_iter().enumerate() {
            let got = cloud.scale_opacity[0].scale[axis];
            assert!(
                (got - expected).abs() <= expected * 1e-6,
                "axis {axis}: got {got}, expected {expected}"
            );
        }
    }

    #[test]
    fn non_finite_scale_codebook_entries_are_rejected() {
        for log_scale in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (mut meta, files) = fixture(1, false);
            meta.scales.codebook[20] = log_scale;
            let err = decode_sog(&meta, &files).unwrap_err();
            assert!(
                matches!(
                    &err,
                    SogError::Field { field: "scales.codebook", reason }
                        if reason.contains("index 20") && reason.contains("splat 0")
                            && reason.contains("axis 1")
                ),
                "unexpected error for {log_scale}: {err}"
            );
        }
    }

    #[test]
    fn overflowing_or_underflowing_scale_exponents_are_rejected() {
        for log_scale in [1000.0, -1000.0] {
            let (mut meta, files) = fixture(1, false);
            meta.scales.codebook[20] = log_scale;
            let err = decode_sog(&meta, &files).unwrap_err();
            assert!(
                matches!(
                    &err,
                    SogError::Field { field: "scales.codebook", reason }
                        if reason.contains("index 20") && reason.contains("splat 0")
                            && reason.contains("axis 1")
                ),
                "unexpected error for {log_scale}: {err}"
            );
        }
    }

    #[test]
    fn missing_scale_codebook_index_is_rejected() {
        let (mut meta, files) = fixture(1, false);
        meta.scales.codebook.truncate(20);
        let err = decode_sog(&meta, &files).unwrap_err();
        assert!(
            matches!(
                &err,
                SogError::Field { field: "scales.codebook", reason }
                    if reason.contains("index 20") && reason.contains("splat 0")
                        && reason.contains("axis 1")
            ),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn trailing_pixels_beyond_count_are_ignored() {
        let (meta, files) = fixture(3, false);
        let cloud = decode_sog(&meta, &files).unwrap();

        // padded to a multiple of 32; splat 3 (the 9.9 trailing pixel) must be
        // a default/empty gaussian, not decoded data
        assert_eq!(cloud.position_visibility.len() % 32, 0);
        assert_eq!(cloud.position_visibility[3].position, [0.0, 0.0, 0.0]);
        assert_eq!(cloud.scale_opacity[3].opacity, 0.0);
    }

    #[test]
    fn sh_n_palette_decodes_or_zero_fills() {
        let (meta, files) = fixture(3, true);
        let cloud = decode_sog(&meta, &files).unwrap();

        if SH_COEFF_COUNT >= 12 {
            // codebook: idx * 0.005 for centroid bytes [10, 20, 30] at
            // interleaved indices (coeff+1)*3 + 0
            for (coeff, byte) in [10.0f32, 20.0, 30.0].iter().enumerate() {
                let idx = (coeff + 1) * SH_CHANNELS;
                let got = cloud.spherical_harmonic[0].coefficients[idx];
                let expected = byte * 0.005;
                assert!(
                    (got - expected).abs() < 1e-6,
                    "coeff {coeff}: got {got}, expected {expected}"
                );
            }
        } else {
            // sh0-only build: higher bands must stay zero-filled, load succeeds
            for idx in SH_CHANNELS..SH_COEFF_COUNT {
                assert_eq!(cloud.spherical_harmonic[0].coefficients[idx], 0.0);
            }
        }
    }

    #[test]
    fn decode_file_selection_omits_unused_bands_but_preserves_inventory() {
        let (mut meta, files) = fixture(3, true);
        assert_eq!(meta.referenced_files().len(), 7);
        let needed = meta.required_files_for_decode(SH_COEFF_COUNT_PER_CHANNEL);
        let expected = if crate::material::spherical_harmonics::SH_DEGREE == 0 {
            5
        } else {
            7
        };
        assert_eq!(needed.len(), expected);
        let required_only = needed
            .into_iter()
            .map(|name| (name.to_owned(), files[name].clone()))
            .collect();
        let cloud = decode_sog(&meta, &required_only).expect("selected files must suffice");
        assert_eq!(cloud.scale_opacity[0].opacity, 0.8);

        meta.sh_n.as_mut().unwrap().bands = 0;
        assert_eq!(
            meta.required_files_for_decode(SH_COEFF_COUNT_PER_CHANNEL)
                .len(),
            5
        );
        assert_eq!(
            meta.referenced_files().len(),
            7,
            "inventory must still report all declared assets"
        );
    }

    #[test]
    fn higher_band_files_are_required_only_when_compiled_degree_uses_them() {
        let (meta, mut files) = fixture(3, true);
        files.remove("shN_centroids.webp");
        files.remove("shN_labels.webp");
        let result = decode_sog(&meta, &files);
        if crate::material::spherical_harmonics::SH_DEGREE == 0 {
            let cloud = result.expect("SH0 does not consume higher-band textures");
            assert_eq!(cloud.scale_opacity[0].opacity, 0.8);
            assert_eq!(cloud.spherical_harmonic[0].coefficients[3], 0.0);
        } else {
            assert!(
                matches!(result, Err(SogError::MissingFile(name)) if name == "shN_centroids.webp")
            );
        }
    }

    #[test]
    fn missing_file_error_names_the_file() {
        let (meta, mut files) = fixture(3, false);
        files.remove("means_u.webp");
        let err = decode_sog(&meta, &files).unwrap_err();
        assert!(matches!(&err, SogError::MissingFile(f) if f == "means_u.webp"));
        assert!(err.to_string().contains("means_u.webp"));
    }

    #[test]
    fn malformed_meta_is_rejected() {
        assert!(matches!(
            parse_meta(br#"{"version": 3, "count": 1}"#),
            Err(SogError::UnsupportedVersion(3))
        ));
        // legacy v1 (no version field)
        assert!(matches!(
            parse_meta(br#"{"count": 1}"#),
            Err(SogError::UnsupportedVersion(1))
        ));
        assert!(matches!(parse_meta(b"not json"), Err(SogError::Meta(_))));
    }

    #[test]
    fn texture_too_small_is_rejected() {
        let (_, files) = fixture(3, false);
        let meta = parse_meta(meta_json(5, false).as_bytes()).unwrap();
        let err = decode_sog(&meta, &files).unwrap_err();
        assert!(matches!(
            err,
            SogError::TextureTooSmall {
                needed: 5,
                got: 4,
                ..
            }
        ));
    }

    #[test]
    fn bundled_sog_does_not_extract_unreferenced_payloads() {
        let bytes = bundled_fixture(false, Some("unused.bin"));
        let cloud = decode_sog_bundle(&bytes).expect("unused payload CRC must never be read");
        assert_eq!(cloud.scale_opacity[0].opacity, 0.8);
    }

    #[test]
    fn bundled_sog_reads_higher_bands_only_when_compiled_degree_uses_them() {
        let bytes = bundled_fixture(true, Some("shN_centroids.webp"));
        let result = decode_sog_bundle(&bytes);
        if SH_COEFF_COUNT_PER_CHANNEL == 1 {
            let cloud = result.expect("SH0 must not extract higher-band payloads");
            assert_eq!(cloud.spherical_harmonic[0].coefficients[3], 0.0);
            assert_eq!(cloud.scale_opacity[0].opacity, 0.8);
        } else {
            assert!(
                matches!(result, Err(SogError::ZipEntry { file, .. }) if file == "shN_centroids.webp")
            );
        }
    }

    #[test]
    fn bundled_sog_preserves_required_payload_checksum_errors() {
        let bytes = bundled_fixture(false, Some("means_u.webp"));
        assert!(
            matches!(decode_sog_bundle(&bytes), Err(SogError::ZipEntry { file, .. }) if file == "means_u.webp")
        );
    }

    #[test]
    fn bundled_sog_supports_nested_metadata_after_textures() {
        let bytes = bundled_fixture(true, None);
        let bundled = decode_sog_bundle(&bytes).expect("nested bundle");
        let (meta, files) = fixture(3, true);
        let unbundled = decode_sog(&meta, &files).expect("unbundled fixture");
        for i in 0..3 {
            assert_eq!(
                bundled.position_visibility[i].position,
                unbundled.position_visibility[i].position
            );
            assert_eq!(
                bundled.spherical_harmonic[i].coefficients,
                unbundled.spherical_harmonic[i].coefficients
            );
            assert_eq!(
                bundled.scale_opacity[i].opacity,
                unbundled.scale_opacity[i].opacity
            );
        }
    }

    // Real ZIP bytes; corrupting a stored payload leaves its directory CRC intact.
    // Reading that entry must fail, making accidental extraction observable.
    fn bundled_fixture(higher_bands: bool, corrupt: Option<&str>) -> Vec<u8> {
        use std::io::{Cursor, Write};
        use zip::write::SimpleFileOptions;

        let (_, mut files) = fixture(3, higher_bands);
        files.insert("unused.bin".into(), vec![42; 1024 * 1024]);
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, data) in files {
            writer
                .start_file(format!("scene/{name}"), options)
                .expect("entry");
            writer.write_all(&data).expect("payload");
        }
        writer
            .start_file("scene/meta.json", options)
            .expect("metadata entry");
        writer
            .write_all(meta_json(3, higher_bands).as_bytes())
            .expect("metadata");
        let mut bytes = writer.finish().expect("archive").into_inner();
        if let Some(name) = corrupt {
            let offset = {
                let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).expect("archive index");
                archive
                    .by_name(&format!("scene/{name}"))
                    .expect("corrupt target")
                    .data_start() as usize
            };
            bytes[offset] ^= 1;
        }
        bytes
    }

    #[test]
    fn bundled_sog_round_trips() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let (_, files) = fixture(3, false);

        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buf);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("meta.json", options).unwrap();
            writer.write_all(meta_json(3, false).as_bytes()).unwrap();
            for (name, bytes) in &files {
                writer.start_file(name.as_str(), options).unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }

        let bundled = decode_sog_bundle(buf.get_ref()).unwrap();
        let (meta, files) = fixture(3, false);
        let unbundled = decode_sog(&meta, &files).unwrap();

        assert_eq!(
            bundled.position_visibility.len(),
            unbundled.position_visibility.len()
        );
        for i in 0..3 {
            assert_eq!(
                bundled.position_visibility[i].position,
                unbundled.position_visibility[i].position
            );
        }
    }
}
