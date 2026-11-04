use bevy_gaussian_splatting::{
    io::sog::from_expanded, material::spherical_harmonics::SH_COEFF_COUNT,
};
use sog_codec::expanded::Expanded;

fn expanded() -> Expanded {
    // Include signed zero and a NaN payload: handoff must preserve every bit.
    let values = |len| {
        (0..len)
            .map(|index| f32::from_bits([0, 0x8000_0000, 0x7fc0_0123, 0x3f80_0000][index % 4]))
            .collect()
    };
    Expanded {
        positions: values(32 * 4),
        rotations: values(32 * 4),
        scales: values(32 * 4),
        harmonics: values(32 * SH_COEFF_COUNT),
    }
}

fn check_handoff(expanded: Expanded, reuse: bool) {
    let pointers = [
        expanded.positions.as_ptr(),
        expanded.rotations.as_ptr(),
        expanded.scales.as_ptr(),
        expanded.harmonics.as_ptr(),
    ];
    let expected = [
        expanded.positions.clone(),
        expanded.rotations.clone(),
        expanded.scales.clone(),
        expanded.harmonics.clone(),
    ];
    let cloud = from_expanded(expanded);
    let actual: [&[f32]; 4] = [
        bytemuck::cast_slice(&cloud.position_visibility),
        bytemuck::cast_slice(&cloud.rotation),
        bytemuck::cast_slice(&cloud.scale_opacity),
        bytemuck::cast_slice(&cloud.spherical_harmonic),
    ];
    for ((plane, expected), pointer) in actual.into_iter().zip(expected).zip(pointers) {
        assert_eq!(
            bytemuck::cast_slice::<f32, u8>(plane),
            bytemuck::cast_slice::<f32, u8>(&expected)
        );
        if reuse {
            assert_eq!(
                plane.as_ptr(),
                pointer,
                "compatible storage must transfer ownership"
            );
        } else {
            assert_ne!(
                plane.as_ptr(),
                pointer,
                "incompatible storage must be copied"
            );
        }
    }
}

#[test]
fn compatible_allocations_transfer_without_changing_bytes() {
    check_handoff(expanded(), true);
}

#[test]
fn indivisible_spare_capacity_preserves_bytes() {
    let mut expanded = expanded();
    for plane in [
        &mut expanded.positions,
        &mut expanded.rotations,
        &mut expanded.scales,
        &mut expanded.harmonics,
    ] {
        let mut spare = Vec::with_capacity(plane.len() + 1);
        spare.extend_from_slice(plane);
        assert_ne!(
            spare.capacity() % 4,
            0,
            "fixture must force incompatible Vec capacity"
        );
        *plane = spare;
    }
    check_handoff(expanded, false);
}

#[test]
#[should_panic]
fn mismatched_plane_length_is_rejected() {
    let mut expanded = expanded();
    expanded.rotations.pop();
    let _ = from_expanded(expanded);
}

#[test]
fn whole_file_decode_reports_real_stages_without_changing_output() {
    use crate::loading::WholeFileTimings;
    let bytes = include_bytes!("../tests/fixtures/tiny.sog");
    let metrics = WholeFileTimings::enabled();
    let decoded = super::decode_for_device_timed("tiny.sog", bytes, None, &metrics).unwrap();
    let expected = super::decode_splat("tiny.sog", bytes).unwrap();
    assert_eq!(decoded.count, expected.count);
    let actual_planes: [&[u8]; 4] = [
        bytemuck::cast_slice(&decoded.cloud.position_visibility),
        bytemuck::cast_slice(&decoded.cloud.rotation),
        bytemuck::cast_slice(&decoded.cloud.scale_opacity),
        bytemuck::cast_slice(&decoded.cloud.spherical_harmonic),
    ];
    let expected_planes: [&[u8]; 4] = [
        bytemuck::cast_slice(&expected.cloud.position_visibility),
        bytemuck::cast_slice(&expected.cloud.rotation),
        bytemuck::cast_slice(&expected.cloud.scale_opacity),
        bytemuck::cast_slice(&expected.cloud.spherical_harmonic),
    ];
    assert_eq!(actual_planes, expected_planes);
    assert_eq!(decoded.bounds, expected.bounds);
    let stages = metrics.report().stages.unwrap();
    assert_eq!(stages.input_validation.attempts, 1);
    assert_eq!(stages.archive_read.attempts, 1);
    assert_eq!(stages.allocation.attempts, 1);
    assert!(stages.texture_decode.attempts >= 4);
    assert!(stages.attribute_expansion.attempts >= 4);
    assert_eq!(stages.plane_transfer.attempts, 1);
    assert_eq!(stages.validation_bounds.attempts, 1);
    assert_eq!(stages.ply_decode.attempts, 0);
    assert!(stages.ply_decode.total_ms.is_none());
    assert!(metrics.report().gpu_upload_elapsed_ms.is_none());
}

#[test]
fn failed_whole_file_validation_is_measured_without_fabricating_decode_work() {
    let metrics = crate::loading::WholeFileTimings::enabled();
    assert!(super::decode_for_device_timed("bad.sog", b"broken", None, &metrics).is_err());
    let stages = metrics.report().stages.unwrap();
    assert_eq!(stages.input_validation.attempts, 1);
    assert!(stages.input_validation.total_ms.is_some());
    assert_eq!(stages.texture_decode.attempts, 0);
    assert!(stages.texture_decode.total_ms.is_none());
}

#[test]
fn rejected_ply_preflight_does_not_report_payload_decode_work() {
    let valid = include_str!("../tests/fixtures/tiny.ply");
    for (bytes, limit, message) in [
        ("broken".to_owned(), None, "header"),
        (valid.replace("vertex 8", "vertex 0"), None, "nonzero"),
        (valid.to_owned(), Some(0), "storage buffer"),
        (
            valid.replace("vertex 8", "vertex 999999"),
            None,
            "payload is too small",
        ),
        (valid.replace("property float x\n", ""), None, "missing x"),
    ] {
        let metrics = crate::loading::WholeFileTimings::enabled();
        let error = super::decode_for_device_timed("bad.ply", bytes.as_bytes(), limit, &metrics)
            .err()
            .expect("preflight must reject fixture");
        assert!(error.contains(message), "{error}");
        let stages = metrics.report().stages.unwrap();
        assert_eq!(stages.input_validation.attempts, 1, "{error}");
        assert!(stages.input_validation.total_ms.is_some());
        assert_eq!(stages.ply_decode.attempts, 0, "{error}");
        assert!(stages.ply_decode.total_ms.is_none());
        assert_eq!(stages.validation_bounds.attempts, 0);
    }
}

#[test]
fn ply_payload_decode_is_measured_after_successful_preflight() {
    let valid = include_str!("../tests/fixtures/tiny.ply");
    for (bytes, success) in [
        (valid.to_owned(), true),
        (valid.replace("-1 -1 -1", "invalid -1 -1"), false),
    ] {
        let metrics = crate::loading::WholeFileTimings::enabled();
        let result = super::decode_for_device_timed("tiny.ply", bytes.as_bytes(), None, &metrics);
        assert_eq!(result.is_ok(), success);
        let stages = metrics.report().stages.unwrap();
        assert_eq!(stages.input_validation.attempts, 1);
        assert_eq!(stages.ply_decode.attempts, 1);
        assert!(stages.ply_decode.total_ms.is_some());
        assert_eq!(stages.validation_bounds.attempts, u64::from(success));
    }
}
