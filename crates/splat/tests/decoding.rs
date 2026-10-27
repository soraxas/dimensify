#![cfg(feature = "render")]

use dimensify_splat::decode_splat;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

fn ply(vertex: &str) -> Vec<u8> {
    let fields = [
        "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "scale_0", "scale_1", "scale_2", "opacity",
        "rot_0", "rot_1", "rot_2", "rot_3",
    ];
    format!(
        "ply\nformat ascii 1.0\nelement vertex 1\n{}end_header\n{vertex}\n",
        fields
            .iter()
            .map(|p| format!("property float {p}\n"))
            .collect::<String>()
    )
    .into_bytes()
}

fn sog(missing: bool) -> Vec<u8> {
    sog_with_count(missing, 1)
}

fn sog_with_count(missing: bool, count: usize) -> Vec<u8> {
    let metadata = serde_json::json!({
        "version": 2, "count": count,
        "means": {"mins": [0.,0.,0.], "maxs": [0.,0.,0.], "files": ["means_l.webp", "means_u.webp"]},
        "scales": {"codebook": vec![-2.;256], "files": ["scales.webp"]},
        "quats": {"files": ["quats.webp"]},
        "sh0": {"codebook": vec![0.;256], "files": ["sh0.webp"]}
    });
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    zip.start_file("scene/meta.json", options).unwrap();
    zip.write_all(metadata.to_string().as_bytes()).unwrap();
    for (name, pixel) in [
        ("means_l.webp", [0, 0, 0, 255]),
        ("means_u.webp", [0, 0, 0, 255]),
        ("scales.webp", [0, 0, 0, 255]),
        ("quats.webp", [128, 128, 128, 252]),
        ("sh0.webp", [0, 0, 0, 204]),
    ] {
        if missing && name == "quats.webp" {
            continue;
        }
        let mut bytes = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .encode(&pixel, 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        zip.start_file(format!("scene/{name}"), options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn gaussian_ply_decodes_authored_position_scale_opacity_and_count() {
    let decoded = decode_splat("scene.PLY", &ply("1 2 3 0 0 0 -2 -2 -2 0 1 0 0 0")).unwrap();
    assert_eq!(decoded.count, 1);
    assert_eq!(decoded.cloud.position_visibility[0].position, [1., 2., 3.]);
    assert_eq!(decoded.bounds.0.to_array(), [1., 2., 3.]);
    assert!((decoded.cloud.scale_opacity[0].scale[0] - (-2_f32).exp()).abs() < 1e-6);
    assert!((decoded.cloud.scale_opacity[0].opacity - 0.5).abs() < 1e-6);
}

#[test]
fn bundled_sog_decodes_nested_metadata_and_opacity() {
    let decoded = decode_splat("scene.sog", &sog(false)).unwrap();
    assert_eq!(decoded.count, 1);
    assert_eq!(decoded.cloud.position_visibility[0].position, [0., 0., 0.]);
    assert!((decoded.cloud.scale_opacity[0].opacity - 0.8).abs() < 1e-6);
    assert!((decoded.cloud.scale_opacity[0].scale[0] - (-2_f32).exp()).abs() < 1e-6);
}

#[test]
fn ordinary_point_clouds_are_rejected_with_a_gaussian_format_error() {
    let error = decode_splat("cloud.ply", b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n0 0 0\n").err().unwrap();
    assert!(error.contains("Gaussian"), "{error}");
}

#[test]
fn malformed_ply_and_incomplete_sog_return_errors() {
    assert!(decode_splat("scene.ply", b"broken").is_err());
    assert!(decode_splat("scene.sog", &sog(true)).is_err());
    assert!(decode_splat("scene.txt", &ply("1 2 3 0 0 0 -2 -2 -2 0 1 0 0 0")).is_err());
}

#[test]
fn non_finite_geometry_and_zero_quaternions_are_rejected() {
    for vertex in [
        "NaN 2 3 0 0 0 -2 -2 -2 0 1 0 0 0",
        "1 2 3 0 0 0 -2 -2 -2 0 0 0 0 0",
    ] {
        assert!(decode_splat("scene.ply", &ply(vertex)).is_err());
    }
}

#[test]
fn oversized_declared_cloud_is_rejected_before_payload_allocation() {
    let bytes = String::from_utf8(ply("1 2 3 0 0 0 -2 -2 -2 0 1 0 0 0"))
        .unwrap()
        .replace("vertex 1", "vertex 999999999");
    let error = decode_splat("large.ply", bytes.as_bytes()).err().unwrap();
    assert!(error.contains("payload"), "{error}");
}

#[test]
fn large_sog_count_is_checked_against_texture_capacity() {
    // Five million is valid; this deliberately incomplete input has only one pixel.
    let error = decode_splat("large.sog", &sog_with_count(false, 5_000_000))
        .err()
        .unwrap();
    assert!(
        error.contains("texture") && error.contains("5000000"),
        "{error}"
    );
}

#[test]
fn zero_and_overflowing_counts_are_rejected_before_allocation() {
    for count in [0, usize::MAX] {
        assert!(decode_splat("invalid.sog", &sog_with_count(false, count)).is_err());
    }
}

#[test]
#[ignore = "set DIMENSIFY_SPLAT_FILE to a local large SOG for acceptance"]
fn local_large_sog_decodes_without_a_fixed_count_cap() {
    let path = std::env::var("DIMENSIFY_SPLAT_FILE").expect("set DIMENSIFY_SPLAT_FILE");
    let bytes = std::fs::read(&path).unwrap();
    let decoded = decode_splat(&path, &bytes).unwrap();
    assert_eq!(decoded.count, 5_000_000);
    assert!(decoded.bounds.0.is_finite() && decoded.bounds.1.is_finite());
}

#[test]
fn sh1_ply_maps_channel_major_coefficients_to_renderer_order() {
    let mut bytes = String::from_utf8(ply(
        "1 2 3 0 0 0 -2 -2 -2 0 1 0 0 0 10 11 12 13 14 15 16 17 18",
    ))
    .unwrap();
    let properties = (0..9)
        .map(|i| format!("property float f_rest_{i}\n"))
        .collect::<String>();
    bytes = bytes.replace("end_header", &format!("{properties}end_header"));
    let decoded = decode_splat("sh1.ply", bytes.as_bytes()).unwrap();
    let coefficients = &decoded.cloud.spherical_harmonic[0].coefficients;
    assert_eq!(&coefficients[3..6], &[10., 13., 16.]);
    assert_eq!(&coefficients[9..12], &[12., 15., 18.]);
}

#[test]
fn binary_little_endian_gaussian_ply_uses_the_same_loader() {
    let ascii = ply("1 2 3 0 0 0 -2 -2 -2 0 1 0 0 0");
    let header = String::from_utf8(ascii)
        .unwrap()
        .split("end_header\n")
        .next()
        .unwrap()
        .replace("format ascii", "format binary_little_endian");
    let mut binary = format!("{header}end_header\n").into_bytes();
    for value in [1_f32, 2., 3., 0., 0., 0., -2., -2., -2., 0., 1., 0., 0., 0.] {
        binary.extend(value.to_le_bytes());
    }
    let decoded = decode_splat("binary.ply", &binary).unwrap();
    assert_eq!(decoded.count, 1);
    assert_eq!(decoded.cloud.position_visibility[0].position, [1., 2., 3.]);
}

#[test]
fn public_load_request_completes_without_a_window_or_gpu() {
    use bevy::prelude::*;
    use dimensify_splat::{LoadPhase, LoadSplat, SourceUp, SplatScene, SplatScenePlugin};
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SplatScenePlugin));
    app.world_mut().write_message(LoadSplat {
        name: "cube.sog".into(),
        bytes: include_bytes!("fixtures/tiny.sog").as_slice().into(),
        up: SourceUp::Z,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.update();
        if let LoadPhase::Ready { name, count } = &app.world().resource::<SplatScene>().phase {
            assert_eq!(name, "cube.sog");
            assert_eq!(*count, 8);
            assert!(app.world().resource::<SplatScene>().loaded.is_some());
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "load did not complete: {:?}",
            app.world().resource::<SplatScene>().phase
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
