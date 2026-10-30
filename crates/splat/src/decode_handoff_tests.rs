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
