use super::*;

fn webp(pixels: &[[u8; 4]]) -> Vec<u8> {
    let pixels = if pixels.is_empty() {
        &[[0; 4]][..]
    } else {
        pixels
    };
    let rgba: Vec<_> = pixels.iter().flatten().copied().collect();
    let mut encoded = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
        .encode(
            &rgba,
            pixels.len() as u32,
            1,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    encoded
}

fn fixture(scales: &[[u8; 4]], codebook: Vec<f32>) -> (SogMeta, HashMap<String, Vec<u8>>) {
    let meta = SogMeta {
        version: 2,
        count: scales.len(),
        means: SogMeans {
            mins: [0.; 3],
            maxs: [0.; 3],
            files: vec!["lo.webp".into(), "hi.webp".into()],
        },
        quats: SogFiles {
            files: vec!["quats.webp".into()],
        },
        scales: SogCodebook {
            codebook,
            files: vec!["scales.webp".into()],
        },
        sh0: SogCodebook {
            codebook: vec![0.],
            files: vec!["sh0.webp".into()],
        },
        sh_n: None,
    };
    let empty = vec![[0; 4]; scales.len()];
    let quats = vec![[128, 128, 128, 252]; scales.len()];
    let files = HashMap::from([
        ("lo.webp".into(), webp(&empty)),
        ("hi.webp".into(), webp(&empty)),
        ("quats.webp".into(), webp(&quats)),
        ("scales.webp".into(), webp(scales)),
        ("sh0.webp".into(), webp(&empty)),
    ]);
    (meta, files)
}

#[test]
fn expanded_scales_match_every_palette_entry_bit_for_bit() {
    let palette: Vec<f32> = (0..256).map(|i| -100. + i as f32 * 0.7).collect();
    let pixels: Vec<_> = (0..256)
        .map(|i| [i as u8, (i + 85) as u8, (i + 170) as u8, 255])
        .collect();
    let (meta, files) = fixture(&pixels, palette.clone());
    let cloud = decode_expanded(&meta, &files, 16).unwrap();
    for (row, pixel) in pixels.iter().enumerate() {
        for (axis, index) in pixel[..3].iter().enumerate() {
            assert_eq!(
                cloud.scales[row * 4 + axis].to_bits(),
                palette[*index as usize].exp().to_bits()
            );
        }
    }
    assert!(
        cloud.scales[pixels.len() * 4..]
            .iter()
            .all(|value| value.to_bits() == 0)
    );
}

#[test]
fn unused_invalid_scales_and_unaddressable_palette_entries_remain_legal() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1000., -1000.] {
        let mut palette = vec![bad; 257];
        palette[0] = -12.;
        palette[1] = 0.;
        palette[2] = 1.;
        let (meta, files) = fixture(&[[0, 1, 2, 255]], palette);
        let cloud = decode_expanded(&meta, &files, 16).unwrap();
        assert_eq!(cloud.scales[..3], [(-12.0f32).exp(), 1., 1.0f32.exp()]);
    }
}

#[test]
fn invalid_used_scales_keep_the_first_splat_axis_and_reason() {
    for (bad, expected) in [
        (f32::NAN, "log scale must be finite"),
        (f32::INFINITY, "log scale must be finite"),
        (f32::NEG_INFINITY, "log scale must be finite"),
        (1000., "exponentiated scale must be finite and positive"),
        (-1000., "exponentiated scale must be finite and positive"),
    ] {
        let (meta, files) = fixture(&[[0, 0, 0, 255], [0, 1, 0, 255]], vec![0., bad]);
        let error = decode_expanded(&meta, &files, 16).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "SOG meta field 'scales.codebook' is invalid: index 1 (splat 1, axis 1): {expected}"
            )
        );
    }
}

#[test]
fn missing_scale_index_keeps_its_error_and_zero_count_has_no_scale_error() {
    let (meta, files) = fixture(&[[0, 0, 255, 0]], vec![0.]);
    assert_eq!(
        decode_expanded(&meta, &files, 16).unwrap_err().to_string(),
        "SOG meta field 'scales.codebook' is invalid: index 255 (splat 0, axis 2): missing codebook entry"
    );
    let (meta, files) = fixture(&[], vec![f32::NAN]);
    let cloud = decode_expanded(&meta, &files, 16).unwrap();
    assert!(cloud.scales.iter().all(|value| value.to_bits() == 0));
}
