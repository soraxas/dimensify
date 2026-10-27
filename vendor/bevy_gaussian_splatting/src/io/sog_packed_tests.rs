use super::tests::{fixture, webp};
use crate::io::sog_packed::{DecodeLimits, decode};

fn limits() -> DecodeLimits {
    DecodeLimits {
        max_texture_dimension: 4096,
        max_texture_bytes: 1 << 20,
    }
}

#[test]
fn externally_admitted_texture_reuses_the_bounded_copy_allocation() {
    use crate::io::sog_packed::DataTexture;
    let pixels = vec![17; 1024 * 1024];
    let allocation = pixels.as_ptr();
    let texture = DataTexture::from_rgba(512, 512, pixels).unwrap();
    assert_eq!(
        texture.texels().as_ptr(),
        allocation,
        "wrapping admitted bytes must not perform another whole-texture copy"
    );
    assert_eq!(texture.clone().texels().as_ptr(), allocation);
}

fn rgba_fixture(
    higher: bool,
) -> (
    super::SogMeta,
    std::collections::HashMap<String, crate::io::sog_packed::DataTexture>,
) {
    use crate::io::sog_packed::DataTexture;
    let (meta, files) = fixture(3, higher);
    let textures = files
        .into_iter()
        .map(|(name, bytes)| {
            let rgba = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
            (
                name,
                DataTexture::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()).unwrap(),
            )
        })
        .collect();
    (meta, textures)
}

#[test]
fn packed_validation_yields_before_exposing_partial_source() {
    use crate::io::sog_packed::begin_decode_rgba;
    let (meta, textures) = rgba_fixture(false);
    let mut job = begin_decode_rgba(&meta, &textures, 3, limits()).unwrap();
    assert!(job.advance(0).unwrap().is_none());
    assert!(
        job.advance(2).unwrap().is_none(),
        "three source rows cannot fit a two-row step"
    );
    let source = job.advance(1).unwrap().expect("all rows checked");
    assert_eq!(source.count(), 3);
    assert_eq!(source.sh0().texels(), &[0, 100, 255, 204].repeat(4));
    assert!(
        job.advance(1).unwrap().is_none(),
        "completed source is delivered once"
    );
}

#[test]
fn packed_validation_rejects_corruption_in_a_later_batch() {
    use crate::io::sog_packed::{DataTexture, begin_decode_rgba};
    let (meta, mut textures) = rgba_fixture(false);
    let mut quats = textures["quats.webp"].texels().to_vec();
    quats[11] = 0; // invalid tag in third logical row, not padding
    textures.insert(
        "quats.webp".into(),
        DataTexture::from_rgba(2, 2, quats).unwrap(),
    );
    let mut job = begin_decode_rgba(&meta, &textures, 3, limits()).unwrap();
    assert!(job.advance(2).unwrap().is_none());
    assert!(job.advance(1).is_err());
    assert!(
        job.advance(usize::MAX).unwrap().is_none(),
        "failed source is never published"
    );
}

#[test]
fn packed_validation_shares_the_row_allowance_with_higher_order_palette() {
    use crate::io::sog_packed::begin_decode_rgba;
    let (meta, textures) = rgba_fixture(true);
    let mut job = begin_decode_rgba(&meta, &textures, 3, limits()).unwrap();
    assert!(job.advance(4).unwrap().is_none()); // 3 splats + 1 of 3 palette rows
    assert!(job.advance(1).unwrap().is_none());
    let source = job.advance(1).unwrap().expect("palette also validated");
    assert_eq!(source.higher_order().unwrap().centroids().texels()[8], 30);
}

#[test]
fn packed_validation_checks_late_palette_indices_and_ignores_padding() {
    use crate::io::sog_packed::{DataTexture, begin_decode_rgba};
    let (mut meta, mut textures) = rgba_fixture(true);
    meta.sh_n.as_mut().unwrap().codebook.truncate(31);
    let mut pixels = textures["shN_centroids.webp"].texels().to_vec();
    pixels[12] = 255; // padding beyond three logical palette rows is not used
    textures.insert(
        "shN_centroids.webp".into(),
        DataTexture::from_rgba(192, 1, pixels.clone()).unwrap(),
    );
    assert!(
        begin_decode_rgba(&meta, &textures, 3, limits())
            .unwrap()
            .advance(6)
            .unwrap()
            .is_some()
    );
    pixels[8] = 31; // third logical palette row points past the codebook
    textures.insert(
        "shN_centroids.webp".into(),
        DataTexture::from_rgba(192, 1, pixels).unwrap(),
    );
    let mut job = begin_decode_rgba(&meta, &textures, 3, limits()).unwrap();
    assert!(job.advance(5).unwrap().is_none());
    assert!(job.advance(1).is_err());
}

#[test]
fn packed_scale_lookup_rejects_used_overflow_and_underflow_but_not_unused_entries() {
    use crate::io::sog_packed::begin_decode_rgba;
    for bad in [1000.0, -1000.0] {
        let (mut meta, textures) = rgba_fixture(false);
        meta.scales.codebook[0] = bad; // finite, but never referenced
        assert!(
            begin_decode_rgba(&meta, &textures, 3, limits())
                .unwrap()
                .advance(3)
                .unwrap()
                .is_some()
        );
        meta.scales.codebook[10] = bad; // fixture references 10/20/30
        let mut job = begin_decode_rgba(&meta, &textures, 3, limits()).unwrap();
        assert!(job.advance(1).is_err());
        assert!(job.advance(usize::MAX).unwrap().is_none());
    }
}

#[test]
#[ignore = "exports byte-exact WebP oracle to SOG_CODEC_ORACLE_DIR for browser validation"]
fn export_browser_codec_oracle() {
    use image::ImageDecoder;
    let root =
        std::path::PathBuf::from(std::env::var("SOG_CODEC_ORACLE_DIR").expect("output directory"));
    std::fs::create_dir_all(&root).unwrap();
    // RGB under zero/tiny alpha is data, not invisible colour to discard.
    let pixels: Vec<[u8; 4]> = (0..=255).map(|alpha| [193, 17, 251, alpha]).collect();
    let encoded = webp(&pixels, 16, 16);
    let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&encoded)).unwrap();
    assert_eq!(decoder.dimensions(), (16, 16));
    let decoded = image::DynamicImage::from_decoder(decoder)
        .unwrap()
        .into_rgba8()
        .into_raw();
    assert_eq!(decoded, pixels.into_iter().flatten().collect::<Vec<_>>());
    std::fs::write(root.join("alpha.webp"), encoded).unwrap();
    std::fs::write(root.join("alpha.rgba"), decoded).unwrap();
}

#[test]
fn packed_sources_preserve_data_texels_and_logical_count_without_float_expansion() {
    let (meta, files) = fixture(3, false);
    let source = decode(&meta, &files, 0, limits()).expect("legal source");
    assert_eq!(source.count(), 3);
    assert_eq!(source.sh0().dimensions(), [2, 2]);
    assert_eq!(source.sh0().texels(), &[0, 100, 255, 204].repeat(4));
    assert_eq!(source.texture_bytes(), 80); // five 2x2 RGBA8 textures, not float clouds
    assert!(source.higher_order().is_none());
}

#[test]
fn externally_decoded_texels_obey_the_same_data_and_admission_contract() {
    use crate::io::sog_packed::{DataTexture, decode_rgba};
    let (meta, files) = fixture(3, true);
    let textures = files
        .iter()
        .map(|(name, bytes)| {
            let rgba = image::load_from_memory_with_format(bytes, image::ImageFormat::WebP)
                .unwrap()
                .into_rgba8();
            (
                name.clone(),
                DataTexture::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()).unwrap(),
            )
        })
        .collect();
    let expected = decode(&meta, &files, 3, limits()).unwrap();
    let actual = decode_rgba(&meta, &textures, 3, limits()).unwrap();
    assert_eq!(actual.texture_bytes(), expected.texture_bytes());
    assert_eq!(actual.sh0().texels(), expected.sh0().texels());
    assert_eq!(
        actual.higher_order().unwrap().centroids().texels(),
        expected.higher_order().unwrap().centroids().texels()
    );
    assert!(DataTexture::from_rgba(1, 1, vec![0; 3]).is_err());
    assert!(DataTexture::from_rgba(0, 1, vec![]).is_err());
    assert!(
        decode_rgba(
            &meta,
            &textures,
            3,
            DecodeLimits {
                max_texture_bytes: actual.texture_bytes() - 1,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        decode_rgba(
            &meta,
            &textures,
            3,
            DecodeLimits {
                max_texture_dimension: 1,
                ..limits()
            }
        )
        .is_err()
    );
}

#[test]
fn packed_higher_order_palette_is_shared_not_expanded_per_splat() {
    let (meta, files) = fixture(3, true);
    let source = decode(&meta, &files, 3, limits()).expect("legal source");
    let sh = source.higher_order().expect("higher bands");
    assert_eq!(sh.bands(), 1);
    assert_eq!(sh.centroids().dimensions(), [192, 1]);
    assert_eq!(sh.labels().dimensions(), [2, 2]);
    assert_eq!(source.texture_bytes(), 864); // 80 base + 16 labels + 768 palette
    let cloned = source.clone();
    assert!(std::ptr::eq(
        source.sh0().texels().as_ptr(),
        cloned.sh0().texels().as_ptr()
    ));
}

#[test]
fn packed_sh0_does_not_read_unused_higher_order_files() {
    let (meta, mut files) = fixture(3, true);
    files.remove("shN_centroids.webp");
    files.remove("shN_labels.webp");
    assert!(decode(&meta, &files, 0, limits()).is_ok());
    assert!(decode(&meta, &files, 3, limits()).is_err());
}

#[test]
fn packed_decode_rejects_aggregate_texture_budget_before_expanding_attributes() {
    let (meta, files) = fixture(3, false);
    assert!(
        decode(
            &meta,
            &files,
            0,
            DecodeLimits {
                max_texture_bytes: 79,
                ..limits()
            }
        )
        .is_err()
    );
    assert!(
        decode(
            &meta,
            &files,
            0,
            DecodeLimits {
                max_texture_bytes: 80,
                ..limits()
            }
        )
        .is_ok()
    );
    assert!(
        decode(
            &meta,
            &files,
            0,
            DecodeLimits {
                max_texture_dimension: 1,
                ..limits()
            }
        )
        .is_err()
    );
}

#[test]
fn packed_gpu_addressing_rejects_mismatched_base_texture_dimensions() {
    let (meta, mut files) = fixture(3, false);
    files.insert("quats.webp".into(), webp(&[[128, 128, 128, 252]; 4], 4, 1));
    assert!(decode(&meta, &files, 0, limits()).is_err());
}

#[test]
fn packed_decode_rejects_invalid_shader_inputs_without_panicking() {
    let (meta, files) = fixture(3, true);
    let mut invalid = meta.clone();
    invalid.means.files.clear();
    assert!(decode(&invalid, &files, 3, limits()).is_err());
    invalid = meta.clone();
    invalid.means.mins[0] = f64::NAN;
    assert!(decode(&invalid, &files, 3, limits()).is_err());
    invalid = meta.clone();
    invalid.scales.codebook[10] = f32::INFINITY;
    assert!(decode(&invalid, &files, 3, limits()).is_err());
    assert!(decode(&meta, &files, 4, limits()).is_err());
    let mut invalid_files = files.clone();
    invalid_files.insert("shN_labels.webp".into(), webp(&[[1, 0, 0, 0]; 4], 2, 2));
    assert!(decode(&meta, &invalid_files, 3, limits()).is_err());
}

#[test]
fn constant_axis_bounds_decode_constant_even_with_nonzero_quantized_pixels() {
    let (mut meta, files) = fixture(3, false);
    meta.means.mins[0] = 0.0;
    meta.means.maxs[0] = 0.0;
    let reference = super::decode_sog(&meta, &files).unwrap();
    for point in &reference.position_visibility[..3] {
        assert_eq!(
            point.position[0], 0.0,
            "zero-width bounds must not become a unit span"
        );
    }
    let source = decode(&meta, &files, 0, limits()).unwrap();
    assert_eq!(source.means_min()[0], source.means_max()[0]);
}

#[test]
#[ignore = "filesystem-only real scene validation; set DIMENSIFY_SOG_PACKED_FIXTURE to a complete LOD fixture"]
fn packed_real_scene_inventory_has_bounded_quantized_payloads() {
    let root = std::path::PathBuf::from(
        std::env::var_os("DIMENSIFY_SOG_PACKED_FIXTURE")
            .expect("explicit local fixture path required"),
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("lod-meta.json")).unwrap()).unwrap();
    let mut filenames = manifest["filenames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    if let Some(environment) = manifest["environment"].as_str() {
        filenames.push(environment.to_owned());
    }
    assert!(!filenames.is_empty());
    let mut total_splats = 0u64;
    let mut total_texels = 0u64;
    for filename in &filenames {
        let path = root.join(filename);
        let meta = super::parse_meta(&std::fs::read(&path).expect("fixture metadata")).unwrap();
        let directory = path.parent().unwrap();
        let files = meta
            .referenced_files()
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    std::fs::read(directory.join(name)).expect("fixture texture"),
                )
            })
            .collect();
        let source = decode(
            &meta,
            &files,
            3,
            DecodeLimits {
                max_texture_dimension: 8192,
                max_texture_bytes: 256 * 1024 * 1024,
            },
        )
        .unwrap_or_else(|error| panic!("{filename}: {error}"));
        assert_eq!(source.count() as usize, meta.count);
        total_splats += u64::from(source.count());
        total_texels += source.texture_bytes();
        eprintln!(
            "{filename}: splats={} encoded_SH={} RGBA_texels={} GPU_payload={}",
            source.count(),
            source.higher_order().map_or(0, |h| h.encoded_bands()),
            source.texture_bytes(),
            crate::render::sog::GpuSogSource::required_bytes(&source)
        );
    }
    eprintln!(
        "fixture validated: {} files, {total_splats} logical splats, {total_texels} retained texture bytes (all files, not a live residency set)",
        filenames.len()
    );
}

#[test]
fn packed_range_owns_only_selected_texels_and_shares_the_palette() {
    let (meta, files) = fixture(3, true);
    let original = decode(&meta, &files, 3, limits()).unwrap();
    let page = original.copy_range(1..3, limits()).unwrap();
    assert_eq!(page.count(), 2);
    assert_eq!(page.means_lo().dimensions(), [2, 1]);
    for (whole, part) in [
        (original.means_lo(), page.means_lo()),
        (original.means_hi(), page.means_hi()),
        (original.quats(), page.quats()),
        (original.scales(), page.scales()),
        (original.sh0(), page.sh0()),
        (
            original.higher_order().unwrap().labels(),
            page.higher_order().unwrap().labels(),
        ),
    ] {
        assert_eq!(part.texels(), &whole.texels()[4..12]);
        assert_ne!(part.texels().as_ptr(), whole.texels()[4..].as_ptr());
    }
    assert_eq!(page.splat_texture_bytes(), 2 * 6 * 4);
    assert_eq!(page.palette_texture_bytes(), 768);
    assert_eq!(page.texture_bytes(), 816);
    assert_eq!(original.texture_bytes(), 864);
    assert_eq!(
        page.higher_order().unwrap().centroids().texels().as_ptr(),
        original
            .higher_order()
            .unwrap()
            .centroids()
            .texels()
            .as_ptr()
    );
    let expected = page.means_lo().texels().to_vec();
    drop(original);
    assert_eq!(page.means_lo().texels(), expected);
}

#[test]
fn packed_range_rejects_invalid_ranges_and_checks_allocation_before_copy() {
    let (meta, files) = fixture(3, true);
    let source = decode(&meta, &files, 3, limits()).unwrap();
    for range in [0..0, 2..1, 2..4, u32::MAX..u32::MAX] {
        assert!(source.copy_range(range, limits()).is_err());
    }
    assert!(
        source
            .copy_range(
                1..3,
                DecodeLimits {
                    max_texture_bytes: 815,
                    ..limits()
                }
            )
            .is_err()
    );
    assert!(
        source
            .copy_range(
                1..3,
                DecodeLimits {
                    max_texture_bytes: 816,
                    ..limits()
                }
            )
            .is_ok()
    );
    assert!(
        source
            .copy_range(
                1..3,
                DecodeLimits {
                    max_texture_dimension: 191,
                    ..limits()
                }
            )
            .is_err(),
        "shared palette must also fit the device"
    );
    let (meta, files) = fixture(3, false);
    let source = decode(&meta, &files, 0, limits()).unwrap();
    let page = source.copy_range(2..3, limits()).unwrap();
    assert_eq!(page.splat_texture_bytes(), 20);
    assert_eq!(page.palette_texture_bytes(), 0);
    assert_eq!(page.count(), 1);
}

#[test]
fn packed_range_retiles_across_source_rows_and_zeroes_padding() {
    let (mut meta, mut files) = fixture(3, false);
    meta.count = 600;
    let pixels = (0..640)
        .map(|i| [(i % 256) as u8, (i / 256) as u8, 17, 253])
        .collect::<Vec<_>>();
    for name in meta.referenced_files() {
        files.insert(name.to_owned(), webp(&pixels, 32, 20));
    }
    let source = decode(&meta, &files, 0, limits()).unwrap();
    let page = source.copy_range(17..530, limits()).unwrap();
    assert_eq!(page.count(), 513);
    assert_eq!(page.means_lo().dimensions(), [256, 3]);
    assert_eq!(
        &page.means_lo().texels()[..513 * 4],
        &source.means_lo().texels()[17 * 4..530 * 4]
    );
    assert!(
        page.means_lo().texels()[513 * 4..]
            .iter()
            .all(|&byte| byte == 0)
    );
    assert_eq!(page.splat_texture_bytes(), 256 * 3 * 5 * 4);
}

#[test]
#[ignore = "requires DIMENSIFY_SOG_PACKED_FIXTURE and SOG_PAGE_RANGES captured reference/old/coarse ranges"]
fn packed_reference_pages_fit_with_old_coverage_and_full_file_staging() {
    use std::collections::{BTreeMap, BTreeSet};
    let root = std::path::PathBuf::from(std::env::var_os("DIMENSIFY_SOG_PACKED_FIXTURE").unwrap());
    let ranges: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("SOG_PAGE_RANGES").unwrap()).unwrap(),
    )
    .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("lod-meta.json")).unwrap()).unwrap();
    let mut filenames = manifest["filenames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect::<Vec<_>>();
    filenames.push(manifest["environment"].as_str().unwrap());
    const PAGE: u32 = 65_536;
    const CAP: u64 = 256 * 1024 * 1024;
    let mut needed = BTreeMap::<usize, BTreeSet<u32>>::new();
    for kind in ["ranges", "coarse", "old"] {
        for range in ranges[kind].as_array().unwrap() {
            let file = range["file"].as_u64().unwrap() as usize;
            let start = range["offset"].as_u64().unwrap() as u32;
            let count = range["count"].as_u64().unwrap() as u32;
            if count > 0 {
                needed
                    .entry(file)
                    .or_default()
                    .extend(start / PAGE..=(start + count - 1) / PAGE);
            }
        }
    }
    let mut resident = 0;
    let mut peak = 0;
    let mut pages = Vec::new();
    let mut shared_palettes = BTreeSet::new();
    for (file, indices) in &needed {
        let path = root.join(filenames[*file]);
        let meta = super::parse_meta(&std::fs::read(&path).unwrap()).unwrap();
        let files = meta
            .referenced_files()
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    std::fs::read(path.parent().unwrap().join(name)).unwrap(),
                )
            })
            .collect();
        let source = decode(
            &meta,
            &files,
            3,
            DecodeLimits {
                max_texture_dimension: 8192,
                max_texture_bytes: CAP - resident,
            },
        )
        .unwrap();
        let mut staging_live = resident + source.texture_bytes();
        peak = peak.max(staging_live);
        assert!(staging_live <= CAP);
        for index in indices {
            let start = index * PAGE;
            let end = (start + PAGE).min(source.count());
            let page = source
                .copy_range(
                    start..end,
                    DecodeLimits {
                        max_texture_dimension: 8192,
                        // The original already owns the shared palette; only the new
                        // per-splat allocation consumes additional live texture bytes.
                        max_texture_bytes: CAP - staging_live + source.palette_texture_bytes(),
                    },
                )
                .unwrap();
            staging_live += page.splat_texture_bytes();
            peak = peak.max(staging_live);
            assert!(peak <= CAP, "page allocation exceeded staging allowance");
            let byte_range = start as usize * 4..end as usize * 4;
            for (whole, part) in [
                (source.means_lo(), page.means_lo()),
                (source.means_hi(), page.means_hi()),
                (source.quats(), page.quats()),
                (source.scales(), page.scales()),
                (source.sh0(), page.sh0()),
                (
                    source.higher_order().unwrap().labels(),
                    page.higher_order().unwrap().labels(),
                ),
            ] {
                assert_eq!(
                    &part.texels()[..(end - start) as usize * 4],
                    &whole.texels()[byte_range.clone()]
                );
                assert!(
                    part.texels()[(end - start) as usize * 4..]
                        .iter()
                        .all(|&v| v == 0)
                );
            }
            let palette = page.higher_order().unwrap().centroids();
            assert_eq!(
                palette.texels().as_ptr(),
                source.higher_order().unwrap().centroids().texels().as_ptr()
            );
            if shared_palettes.insert(palette.texels().as_ptr() as usize) {
                resident += page.palette_texture_bytes();
            }
            resident += page.splat_texture_bytes();
            pages.push(page);
        }
        assert_eq!(resident, staging_live - source.splat_texture_bytes());
    }
    assert_eq!(needed.len(), shared_palettes.len());
    assert_eq!(
        pages.len(),
        106,
        "recorded old/reference/coarse page union changed"
    );
    assert_eq!(resident, 225_521_664);
    eprintln!(
        "reference pages: pages={}, files={}, retained_texture_bytes={resident}, peak_texture_bytes_including_full_file_staging={peak}",
        pages.len(),
        needed.len()
    );
    if let Ok(path) = std::env::var("SOG_PAGE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({
            "scope": "real decoded CPU pages; not browser, VRAM or process-memory measurement",
            "page_splats": PAGE, "pages": pages.len(), "files": needed.len(),
            "retained_texture_bytes": resident, "peak_texture_bytes_with_full_file_staging": peak,
            "cap": CAP,
        })).unwrap()).unwrap();
    }
}

#[test]
fn refetched_pages_can_reuse_identical_palette_without_pinning_old_planes() {
    let (meta, files) = fixture(3, true);
    let first = decode(&meta, &files, 3, limits()).unwrap();
    let page = first.copy_range(0..1, limits()).unwrap();
    let mut refetched = decode(&meta, &files, 3, limits()).unwrap();
    assert_ne!(
        refetched
            .higher_order()
            .unwrap()
            .centroids()
            .texels()
            .as_ptr(),
        page.higher_order().unwrap().centroids().texels().as_ptr()
    );
    refetched.reuse_palette(&page).unwrap();
    assert_eq!(
        refetched
            .higher_order()
            .unwrap()
            .centroids()
            .texels()
            .as_ptr(),
        page.higher_order().unwrap().centroids().texels().as_ptr()
    );
    let mut altered = meta.clone();
    altered.sh_n.as_mut().unwrap().codebook[0] += 0.25;
    let other = decode(&altered, &files, 3, limits()).unwrap();
    assert!(refetched.reuse_palette(&other).is_err());
}
