use super::*;

fn fixture(count: usize, rest: usize, crlf: bool) -> (Vec<u8>, usize, usize) {
    let mut header = format!("ply\nformat binary_little_endian 1.0\nelement vertex {count}\n");
    let properties = [
        "x",
        "y",
        "z",
        "f_dc_0",
        "f_dc_1",
        "f_dc_2",
        "scale_0",
        "scale_1",
        "scale_2",
        "opacity",
        "rot_0",
        "rot_1",
        "rot_2",
        "rot_3",
        "visibility",
    ];
    for property in properties {
        header.push_str(&format!("property float {property}\n"));
    }
    for i in 0..rest {
        header.push_str(&format!("property float f_rest_{i}\n"));
    }
    // Unknown scalar properties must not change the record stride.
    header.push_str("property uchar label\nproperty double confidence\nend_header\n");
    if crlf {
        header = header.replace('\n', "\r\n");
    }
    let header_len = header.len();
    let mut bytes = header.into_bytes();
    for row in 0..count {
        let values = [
            row as f32,
            -(row as f32),
            0.25,
            0.1,
            -0.2,
            0.3,
            -1.0,
            0.0,
            0.5,
            0.75,
            2.0,
            1.0,
            -2.0,
            0.5,
            0.8,
        ];
        for value in values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for i in 0..rest {
            bytes.extend_from_slice(&(i as f32 * 0.125 - row as f32).to_le_bytes());
        }
        bytes.push(7);
        bytes.extend_from_slice(&0.75_f64.to_le_bytes());
    }
    (bytes, header_len, (properties.len() + rest) * 4 + 9)
}

fn parity(bytes: &[u8], chunks: &[usize]) {
    let expected = crate::decode_splat("test.ply", bytes).unwrap();
    let mut decoder = Decoder::new(None);
    let mut batches = Vec::new();
    let mut offset = 0;
    for &size in chunks {
        batches.extend(decoder.push(&bytes[offset..offset + size]).unwrap());
        offset += size;
    }
    batches.extend(decoder.push(&bytes[offset..]).unwrap());
    decoder.finish().unwrap();
    assert!(!decoder.is_fallback());
    let actual: Vec<_> = batches
        .iter()
        .flat_map(|batch| batch.cloud.iter())
        .collect();
    assert_eq!(
        actual,
        expected
            .cloud
            .iter()
            .take(expected.count)
            .collect::<Vec<_>>()
    );
    assert_eq!(actual.len(), expected.count);
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for batch in batches {
        assert_eq!(batch.total, expected.count);
        assert_eq!(batch.count, batch.cloud.position_visibility.len());
        assert!(batch.count > 0 && batch.count <= 4096);
        min = min.min(batch.bounds.0);
        max = max.max(batch.bounds.1);
        let support = batch
            .cloud
            .scale_opacity
            .iter()
            .flat_map(|v| v.scale)
            .fold(0.0_f32, f32::max)
            * 3.0;
        assert_eq!(batch.support, support);
    }
    assert_eq!((min, max), expected.bounds);
}

#[test]
fn every_header_and_record_split_matches_whole_file_sh3() {
    let (bytes, header, stride) = fixture(3, 45, true);
    for split in 0..=header + stride {
        parity(&bytes, &[split]);
    }
    parity(&bytes, &vec![1; bytes.len()]);
}

#[test]
fn all_sh_degrees_and_bounded_unpadded_batches_match_whole_file() {
    for rest in [0, 9, 24, 45] {
        let (bytes, _, _) = fixture(4101, rest, false);
        parity(&bytes, &[]);
    }
}

#[test]
fn emits_before_completion_and_retains_only_partial_record() {
    let (bytes, header, stride) = fixture(4, 0, false);
    let mut decoder = Decoder::new(None);
    let first = decoder.push(&bytes[..header + stride + 3]).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].count, 1);
    assert!(decoder.finish().is_err());
}

#[test]
fn rejects_truncated_header_record_and_missing_rows() {
    let (bytes, header, stride) = fixture(3, 0, false);
    for end in [0, header - 1, header, header + stride - 1, header + stride] {
        let mut decoder = Decoder::new(None);
        decoder.push(&bytes[..end]).unwrap();
        assert!(decoder.finish().is_err(), "accepted truncation at {end}");
    }
}

#[test]
fn rejects_invalid_attributes_and_zero_quaternion() {
    let (bytes, header, _) = fixture(1, 45, false);
    for (property, value) in [
        (0, f32::NAN),
        (6, 1000.0),
        (7, -1000.0),
        (10, f32::INFINITY),
        (18, f32::NAN),
    ] {
        let mut invalid = bytes.clone();
        invalid[header + property * 4..header + property * 4 + 4]
            .copy_from_slice(&value.to_le_bytes());
        assert!(Decoder::new(None).push(&invalid).is_err());
    }
    let mut invalid = bytes;
    invalid[header + 10 * 4..header + 14 * 4].fill(0);
    assert!(Decoder::new(None).push(&invalid).is_err());
}

#[test]
fn validates_schema_counts_storage_and_header_budget_before_body() {
    let (bytes, header, _) = fixture(1, 0, false);
    let valid = String::from_utf8(bytes[..header].to_vec()).unwrap();
    for invalid in [
        valid.replace("element vertex 1", "element vertex 0"),
        valid.replace("element vertex 1", "element vertex 999999999"),
        valid.replace("property float scale_2\n", ""),
        valid.replace("property float x", "property double x"),
        valid.replace("end_header", "element face 1\nend_header"),
        valid.replace("end_header", "property list uchar int indices\nend_header"),
        valid.replace("end_header", "property float f_rest_2\nend_header"),
    ] {
        assert!(
            Decoder::new(None).push(invalid.as_bytes()).is_err(),
            "accepted {invalid}"
        );
    }
    assert!(Decoder::new(Some(1)).push(valid.as_bytes()).is_err());
    let oversized = format!("ply\ncomment {}", "x".repeat(64 * 1024));
    assert!(Decoder::new(None).push(oversized.as_bytes()).is_err());
}

#[test]
fn unsupported_valid_encodings_request_fallback_before_any_batches() {
    let (bytes, header, _) = fixture(1, 0, false);
    let valid = String::from_utf8(bytes[..header].to_vec()).unwrap();
    for encoding in ["ascii", "binary_big_endian"] {
        let input = valid.replace("binary_little_endian", encoding);
        let mut decoder = Decoder::new(None);
        assert!(decoder.push(input.as_bytes()).unwrap().is_empty());
        assert!(decoder.is_fallback());
        assert!(decoder.push(&[1, 2, 3]).unwrap().is_empty());
        decoder.finish().unwrap();
    }
}

#[test]
fn rejects_payload_after_declared_count() {
    let (mut bytes, _, _) = fixture(1, 0, false);
    bytes.push(0);
    assert!(Decoder::new(None).push(&bytes).is_err());
}

#[test]
fn rejects_header_integer_overflow_without_panicking() {
    let (bytes, header, _) = fixture(1, 0, false);
    let valid = String::from_utf8(bytes[..header].to_vec()).unwrap();
    for invalid in [
        valid.replace("element vertex 1", "element vertex 18446744073709551616"),
        valid.replace("1.0", "18446744073709551616.0"),
        valid.replace("1.0", "18446744073709551616"),
        valid.replace("1.0", "1.18446744073709551616"),
    ] {
        assert!(Decoder::new(None).push(invalid.as_bytes()).is_err());
    }
}
