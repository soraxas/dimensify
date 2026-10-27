#import bevy_render::view::View
@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var attributes: texture_2d<f32>;

struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32,
    @location(0) splat_index: u32) -> Output {
    let row = textureDimensions(attributes).x / 4u;
    let base = vec2<i32>(i32((splat_index % row) * 4u), i32(splat_index / row));
    let center_opacity = textureLoad(attributes, base, 0);
    let a = textureLoad(attributes, base + vec2<i32>(1, 0), 0);
    let b = textureLoad(attributes, base + vec2<i32>(2, 0), 0);
    let color = textureLoad(attributes, base + vec2<i32>(3, 0), 0);
    var out: Output;
    out.position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    out.color = vec4<f32>(0.0);
    out.uv = vec2<f32>(0.0);
    let center = view.clip_from_world * vec4<f32>(center_opacity.xyz, 1.0);
    if center.w <= 0.0 || center.z < 0.0 || center.z > center.w { return out; }
    var t = view.view_from_world * vec4<f32>(center_opacity.xyz, 1.0);
    if view.clip_from_view[3].w == 1.0 { t = vec4<f32>(0.0, 0.0, 1.0, 1.0); }
    let covariance = mat3x3<f32>(a.x, a.y, a.z, a.y, a.w, b.x, a.z, b.x, b.y);
    let focal = vec2<f32>(view.clip_from_view[0].x, view.clip_from_view[1].y) * view.viewport.zw;
    let z2 = t.z * t.z;
    let j = mat3x3<f32>(focal.x / t.z, 0.0, -focal.x * t.x / z2,
        0.0, focal.y / t.z, -focal.y * t.y / z2, 0.0, 0.0, 0.0);
    let w = transpose(mat3x3<f32>(view.view_from_world[0].xyz,
        view.view_from_world[1].xyz, view.view_from_world[2].xyz));
    let transform = w * j;
    let projected = transpose(transform) * covariance * transform;
    let xx = projected[0][0] + 0.3;
    let xy = projected[0][1];
    let yy = projected[1][1] + 0.3;
    let mid = 0.5 * (xx + yy);
    let radius = length(vec2<f32>(0.5 * (xx - yy), xy));
    let major = max(mid + radius, 0.0);
    let minor = max(mid - radius, 0.0);
    var axis = vec2<f32>(1.0, 0.0);
    if xy == 0.0 {
        if yy > xx { axis = vec2<f32>(0.0, 1.0); }
    } else {
        axis = vec2<f32>(major - yy, xy);
        if yy > xx { axis = vec2<f32>(xy, major - xx); }
        axis = normalize(axis / max(abs(axis.x), abs(axis.y)));
    }
#ifdef NORMALIZED_GAUSSIAN
    let cutoff = sqrt(8.0);
#else
    let cutoff = sqrt(max(9.0 + 2.0 * log(center_opacity.w), 0.000001));
#endif
    let cap = 2.0 * min(1024.0, min(view.viewport.z, view.viewport.w));
    let extent = min(cutoff * sqrt(vec2<f32>(major, minor)), vec2<f32>(cap));
    // Triangle strip avoids indices and indirect drawing on WebGL.
    let corner = vec2<f32>(f32(vertex_index & 1u) * 2.0 - 1.0,
        f32((vertex_index >> 1u) & 1u) * 2.0 - 1.0);
    let delta = (axis * corner.x * extent.x + vec2<f32>(axis.y, -axis.x) * corner.y * extent.y) / view.viewport.zw;
    out.position = center + vec4<f32>(delta * center.w, 0.0, 0.0);
    out.uv = corner;
    out.color = vec4<f32>(color.rgb, center_opacity.w);
    return out;
}

@fragment
fn fragment(input: Output) -> @location(0) vec4<f32> {
#ifdef NORMALIZED_GAUSSIAN
    let power = -4.0 * dot(input.uv, input.uv);
    let edge = exp(-4.0);
    let alpha = max(0.0, (exp(power) - edge) / (1.0 - edge)) * input.color.a;
    if power < -4.0 || alpha < 1.0 / 255.0 { discard; }
#else
    let alpha = min(exp(-4.5 * dot(input.uv, input.uv)) * input.color.a, 0.999);
#endif
    return vec4<f32>(input.color.rgb * alpha, alpha);
}
