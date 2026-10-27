// Compact encoding matches PlayCanvas dfcc50fbbfba2388843041a875ec0ef5d1a586c5
// containerCompactWrite (MIT; docs/legal/playcanvas-MIT.txt).
// Copyright (c) 2011-2026 PlayCanvas Ltd. Its planes total
// 24 bytes, despite the upstream comment saying 20. Stored positions stay in
// source space; the consuming projector applies the scene transform once.

struct SogBakeParams {
    source_start: u32,
    count: u32,
    destination_start: u32,
    color_only: u32,
    camera: vec4f,
    ray: vec4f,
}
@group(1) @binding(0) var<storage, read_write> work_transform: array<vec4u>;
@group(1) @binding(1) var<storage, read_write> work_rotation: array<u32>;
@group(1) @binding(2) var<storage, read_write> work_color: array<u32>;
@group(1) @binding(3) var<uniform> bake: SogBakeParams;

fn compact_rgb(color: vec3f) -> u32 {
    let c = clamp(color, vec3f(0.0), vec3f(4.0));
    let q = vec3u(c * vec3f(2047.0, 2047.0, 1023.0) / 4.0 + 0.5);
    return q.x | (q.y << 11u) | (q.z << 22u);
}

fn compact_rotation(rotation: vec4f) -> u32 {
    var q = rotation;
    if q.w < 0.0 { q = -q; }
    let p = q.xyz * inverseSqrt(1.0 + q.w);
    let bits = vec3u(clamp(p * 0.5 + 0.5, vec3f(0.0), vec3f(1.0))
        * vec3f(2047.0, 2047.0, 1023.0) + 0.5);
    return bits.x | (bits.y << 11u) | (bits.z << 22u);
}

fn compact_scale(scale: f32) -> u32 {
    if scale < 1e-10 { return 0u; }
    return u32(clamp((log(scale) + 12.0) * (255.0 / 21.0) + 0.5, 1.0, 255.0));
}

@compute @workgroup_size(64)
fn sog_bake(@builtin(global_invocation_id) id: vec3u) {
    if id.x >= bake.count { return; }
    let index = bake.source_start + id.x;
    let destination = bake.destination_start + id.x;
    if index >= sog_info.count || destination >= arrayLength(&work_transform)
        || destination >= arrayLength(&work_rotation) || destination >= arrayLength(&work_color) { return; }
    var v: SogAttributes;
    if bake.color_only != 0u {
        v.position = bitcast<vec3f>(work_transform[destination].xyz);
    } else {
        v = sog_decode(index);
    }
    var ray = bake.ray.xyz;
    if bake.camera.w != 0.0 {
        let offset = v.position * 0.5 - bake.camera.xyz * 0.5;
        // Camera exactly at the centre has no unique SH direction. Use a stable
        // source-axis fallback rather than normalizing zero into NaNs.
        let largest = max(max(abs(offset.x), abs(offset.y)), abs(offset.z));
        ray = vec3f(0.0, 0.0, 1.0);
        if largest > 1e-10 {
            let direction = offset / largest;
            ray = direction * inverseSqrt(dot(direction, direction));
        }
    }
    work_color[destination] = compact_rgb(sog_color(index, ray));
    if bake.color_only != 0u { return; }
    let packed_scale = compact_scale(v.scale.x) | (compact_scale(v.scale.y) << 8u)
        | (compact_scale(v.scale.z) << 16u) | (u32(clamp(v.opacity, 0.0, 1.0) * 255.0 + 0.5) << 24u);
    work_transform[destination] = vec4u(bitcast<vec3u>(v.position), packed_scale);
    work_rotation[destination] = compact_rotation(v.rotation);
}
