// SOG v2 attribute decode and real SH evaluation, adapted from PlayCanvas
// engine dfcc50fbbfba2388843041a875ec0ef5d1a586c5 (MIT; docs/legal/playcanvas-MIT.txt).
// Copyright (c) 2011-2026 PlayCanvas Ltd.
// Integer texels avoid normalized-float round trips. No ingestion/world transform
// is applied here. Directions are unit camera-to-splat rays in source coordinates.

struct SogInfo {
    means_min: vec4f,
    means_max: vec4f,
    count: u32,
    width: u32,
    bands: u32,
    encoded_coefficients: u32,
}
@group(0) @binding(0) var sog_means_lo: texture_2d<u32>;
@group(0) @binding(1) var sog_means_hi: texture_2d<u32>;
@group(0) @binding(2) var sog_quats: texture_2d<u32>;
@group(0) @binding(3) var sog_scales: texture_2d<u32>;
@group(0) @binding(4) var sog_sh0: texture_2d<u32>;
@group(0) @binding(5) var sog_labels: texture_2d<u32>;
@group(0) @binding(6) var sog_centroids: texture_2d<u32>;
@group(0) @binding(7) var sog_lut: texture_2d<f32>;
@group(0) @binding(8) var<uniform> sog_info: SogInfo;

struct SogAttributes {
    position: vec3f,
    rotation: vec4f, // xyzw, unlike the legacy CPU cloud's wxyz
    scale: vec3f,
    opacity: f32,
}

fn sog_uv(index: u32) -> vec2u {
    return vec2u(index % sog_info.width, index / sog_info.width);
}

fn sog_lookup(indices: vec3u, channel: u32) -> vec3f {
    return vec3f(textureLoad(sog_lut, vec2u(indices.x, 0u), 0)[channel],
        textureLoad(sog_lut, vec2u(indices.y, 0u), 0)[channel],
        textureLoad(sog_lut, vec2u(indices.z, 0u), 0)[channel]);
}

fn sog_decode(index: u32) -> SogAttributes {
    var result: SogAttributes;
    // Padding is transparent; a rounded dispatch must never consume source tails.
    if index >= sog_info.count { return result; }
    let uv = sog_uv(index);
    let fixed = textureLoad(sog_means_lo, uv, 0).xyz
        + 256u * textureLoad(sog_means_hi, uv, 0).xyz;
    let log_position = mix(sog_info.means_min.xyz, sog_info.means_max.xyz, vec3f(fixed) / 65535.0);
    result.position = sign(log_position) * (exp(abs(log_position)) - 1.0);
    let packed_quat = textureLoad(sog_quats, uv, 0);
    let abc = (vec3f(packed_quat.xyz) / 255.0 - 0.5) * 1.4142135623730951;
    let omitted = sqrt(max(0.0, 1.0 - dot(abc, abc)));
    switch packed_quat.w {
        case 252u: { result.rotation = vec4f(abc, omitted); }
        case 253u: { result.rotation = vec4f(omitted, abc.y, abc.z, abc.x); }
        case 254u: { result.rotation = vec4f(abc.y, omitted, abc.z, abc.x); }
        default: { result.rotation = vec4f(abc.y, abc.z, omitted, abc.x); }
    }
    result.scale = exp(sog_lookup(textureLoad(sog_scales, uv, 0).xyz, 0u));
    result.opacity = f32(textureLoad(sog_sh0, uv, 0).w) / 255.0;
    return result;
}

fn sog_coefficient(index: u32, coefficient: u32) -> vec3f {
    if index >= sog_info.count { return vec3f(0.0); }
    let uv = sog_uv(index);
    if coefficient == 0u {
        return sog_lookup(textureLoad(sog_sh0, uv, 0).xyz, 1u);
    }
    let count = (sog_info.bands + 1u) * (sog_info.bands + 1u);
    if coefficient >= count { return vec3f(0.0); }
    let label = textureLoad(sog_labels, uv, 0);
    let palette_index = label.x + label.y * 256u;
    // Stride is the encoded degree, not the requested degree: SH3 palettes still
    // have 15 texels per entry when evaluating only their first band.
    let palette_uv = vec2u((palette_index % 64u) * sog_info.encoded_coefficients
        + coefficient - 1u, palette_index / 64u);
    return sog_lookup(textureLoad(sog_centroids, palette_uv, 0).xyz, 2u);
}

fn sog_color(index: u32, d: vec3f) -> vec3f {
    if index >= sog_info.count { return vec3f(0.0); }
    var color = vec3f(0.5) + 0.28209479177387814 * sog_coefficient(index, 0u);
    if sog_info.bands == 0u { return max(color, vec3f(0.0)); }
    color += 0.4886025119029199 * (-sog_coefficient(index, 1u) * d.y
        + sog_coefficient(index, 2u) * d.z - sog_coefficient(index, 3u) * d.x);
    if sog_info.bands == 1u { return max(color, vec3f(0.0)); }
    let xx = d.x * d.x; let yy = d.y * d.y; let zz = d.z * d.z;
    color += sog_coefficient(index, 4u) * (1.0925484305920792 * d.x * d.y)
        + sog_coefficient(index, 5u) * (-1.0925484305920792 * d.y * d.z)
        + sog_coefficient(index, 6u) * (0.31539156525252005 * (2.0 * zz - xx - yy))
        + sog_coefficient(index, 7u) * (-1.0925484305920792 * d.x * d.z)
        + sog_coefficient(index, 8u) * (0.5462742152960396 * (xx - yy));
    if sog_info.bands == 2u { return max(color, vec3f(0.0)); }
    color += sog_coefficient(index, 9u) * (-0.5900435899266435 * d.y * (3.0 * xx - yy))
        + sog_coefficient(index, 10u) * (2.890611442640554 * d.x * d.y * d.z)
        + sog_coefficient(index, 11u) * (-0.4570457994644658 * d.y * (4.0 * zz - xx - yy))
        + sog_coefficient(index, 12u) * (0.3731763325901154 * d.z * (2.0 * zz - 3.0 * xx - 3.0 * yy))
        + sog_coefficient(index, 13u) * (-0.4570457994644658 * d.x * (4.0 * zz - xx - yy))
        + sog_coefficient(index, 14u) * (1.445305721320277 * d.z * (xx - yy))
        + sog_coefficient(index, 15u) * (-0.5900435899266435 * d.x * (xx - 3.0 * yy));
    return max(color, vec3f(0.0));
}
