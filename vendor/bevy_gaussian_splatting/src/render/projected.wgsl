#import bevy_gaussian_splatting::bindings::{view, gaussian_uniforms}
#import bevy_gaussian_splatting::planar::{get_position, get_opacity, get_color}
#import bevy_gaussian_splatting::gaussian_3d::compute_cov2d_3dgs
#import bevy_gaussian_splatting::helpers::{bound_kernel_covariance, get_bounding_box_clip}
#import bevy_gaussian_splatting::transform::{splat_center_is_projectable, project_splat_center, world_view_direction}

struct ProjectedRecord {
    // xyz carry NDC float bits; w is the integer radial-depth key. Keeping
    // keys integer avoids NaN constants/canonicalization in browser WGSL.
    centre_key: vec4<u32>,
    axes_colour: vec4<u32>,
}
@group(3) @binding(0) var<storage, read_write> output: array<ProjectedRecord>;

@compute @workgroup_size(64)
fn project(@builtin(global_invocation_id) invocation: vec3<u32>,
           @builtin(num_workgroups) groups: vec3<u32>) {
    let id = invocation.y * groups.x * 64u + invocation.x;
    if id >= gaussian_uniforms.count { return; }
    output[id].centre_key = vec4<u32>(0u, 0u, 0u, 0xFFFFFFFFu);
    output[id].axes_colour = vec4<u32>(0u);
    let world = (gaussian_uniforms.transform * vec4<f32>(get_position(id), 1.0)).xyz;
    let alpha = get_opacity(id) * gaussian_uniforms.global_opacity;
    if !splat_center_is_projectable(world) || alpha < 1.0 / 255.0 { return; }
    let centre = project_splat_center(world);
    let cutoff = sqrt(8.0);
    let covariance = bound_kernel_covariance(compute_cov2d_3dgs(world, id), cutoff);
    let x = get_bounding_box_clip(covariance, vec2<f32>(1.0, 0.0), cutoff).xy;
    let y = get_bounding_box_clip(covariance, vec2<f32>(0.0, 1.0), cutoff).xy;
    // Retain overlapping support, not only centre-in-frustum. Inflate by the
    // half-float packing error and one pixel; uncertain/NaN comparisons retain.
    let radius = abs(x) + abs(y);
    let slack = radius * 0.002 + 2.0 / view.viewport.zw;
    if any(abs(centre.xy) - radius - slack > vec2<f32>(1.0)) { return; }
    let colour = get_color(id, world_view_direction(world));
    let delta = world - view.world_position;
    let key = 0xFFFFFFFFu - bitcast<u32>(dot(delta, delta));
    output[id].centre_key = vec4<u32>(bitcast<vec3<u32>>(centre.xyz), key);
    output[id].axes_colour = vec4<u32>(pack2x16float(x), pack2x16float(y), pack2x16float(colour.rg), pack2x16float(vec2<f32>(colour.b, alpha)));
}
