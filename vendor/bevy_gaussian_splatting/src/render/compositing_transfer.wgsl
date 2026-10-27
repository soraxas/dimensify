// Linear <-> encoded premultiplied view-target transfer. No filtering: pixels
// outside the active viewport must survive the round trip unchanged.
#import bevy_gaussian_splatting::spherical_harmonics::{srgb_to_linear, linear_to_srgb}
struct TransferRect {
    bounds: vec4<u32>, // inclusive min XY, exclusive max XY
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> rect: TransferRect;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - vec2<f32>(1.0), 0.0, 1.0);
}

@fragment
fn encode(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return transfer(position, true);
}

@fragment
fn decode(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return transfer(position, false);
}

fn transfer(position: vec4<f32>, encoding: bool) -> vec4<f32> {
    let pixel = vec2<u32>(position.xy);
    let color = textureLoad(source, vec2<i32>(pixel), 0);
    if any(pixel < rect.bounds.xy) || any(pixel >= rect.bounds.zw) || color.a <= 0.0 {
        return color;
    }
    // Convert straight RGB, then restore premultiplication. Alpha and HDR range
    // are not clamped. The low branch also preserves negative linear values.
    let straight = color.rgb / color.a;
    if encoding {
        return vec4<f32>(linear_to_srgb(straight) * color.a, color.a);
    }
    return vec4<f32>(srgb_to_linear(straight) * color.a, color.a);
}
