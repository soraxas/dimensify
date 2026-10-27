#define_import_path bevy_gaussian_splatting::transform

#import bevy_gaussian_splatting::bindings::view

fn world_to_clip(world_pos: vec3<f32>) -> vec4<f32> {
    let homogenous_pos = view.unjittered_clip_from_world * vec4<f32>(world_pos, 1.0);
    return homogenous_pos / (homogenous_pos.w + 0.000000001);
}

fn in_frustum(clip_space_pos: vec3<f32>) -> bool {
    return abs(clip_space_pos.x) < 1.1
        && abs(clip_space_pos.y) < 1.1
        && abs(clip_space_pos.z - 0.5) < 0.5;
}

// Gaussian billboards are not points. Position-only sorting must not discard a
// centre outside XY or the near/far planes: its footprint can still be visible.
// Positive homogeneous W rejects behind-eye perspective centres and accepts
// orthographic centres (W=1). The rasterizer clips the resulting footprint.
fn splat_center_is_projectable(world_pos: vec3<f32>) -> bool {
    let clip = view.unjittered_clip_from_world * vec4<f32>(world_pos, 1.0);
    return clip.w > 0.0;
}

// Call only for projectable centres. Like PlayCanvas gsplatCenter, preserve
// close-up splats by clamping billboard depth, not rejecting their centres.
// Bevy uses reverse-Z in [0,1]; keep W exactly 1 after perspective division.
fn project_splat_center(world_pos: vec3<f32>) -> vec4<f32> {
    let clip = view.unjittered_clip_from_world * vec4<f32>(world_pos, 1.0);
    return vec4<f32>(clip.xy / clip.w, clamp(clip.z / clip.w, 0.0, 1.0), 1.0);
}

// Camera-to-splat direction for directional appearance. Orthographic rays are
// parallel, including at the camera plane where a position difference can be 0.
fn world_view_direction(world_pos: vec3<f32>) -> vec3<f32> {
    if view.clip_from_view[3].w == 1.0 {
        return normalize(-view.world_from_view[2].xyz);
    }
    return normalize(world_pos - view.world_position);
}
