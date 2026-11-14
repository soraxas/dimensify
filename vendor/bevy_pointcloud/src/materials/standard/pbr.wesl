import bevy_pbr::render::pbr_types;
import bevy_pbr::render::pbr_functions::alpha_discard;
import bevy_pbr::render::pbr_fragment::pbr_input_from_standard_material;
import bevy_pbr::decal::clustered::apply_decals;
import bevy_pbr::render::pbr_bindings;
@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::VertexOutput as PbrVertexOutput;
@if(PREPASS_PIPELINE)
import bevy_pbr::prepass::io::FragmentOutput;
@if(PREPASS_PIPELINE)
import bevy_pbr::deferred::functions::deferred_output;
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::forward_io::VertexOutput as PbrVertexOutput;
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::forward_io::FragmentOutput;
@if(!PREPASS_PIPELINE)
import bevy_pbr::render::pbr_functions::main_pass_post_lighting_processing;
@if(!PREPASS_PIPELINE && !POINTCLOUD_UNLIT)
import bevy_pbr::render::pbr_functions::apply_pbr_lighting;
@if(!PREPASS_PIPELINE && !POINTCLOUD_UNLIT)
import bevy_pbr::render::pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
@if(PREPASS_PIPELINE)
import bevy_pointcloud::render::prepass::prepass_io::VertexOutput;
@if(!PREPASS_PIPELINE)
import bevy_pointcloud::assets::shaders::forward_io::VertexOutput;
import bevy_pointcloud::assets::shaders::functions;
import bevy_pointcloud::assets::shaders::pointcloud_bindings::pointcloud;
import bevy_pointcloud::assets::shaders::pointcloud_functions;
@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;
@if(MESHLET_MESH_MATERIAL_PASS)
import bevy_pbr::meshlet::visibility_buffer_resolve::resolve_vertex_output;
@if(MATERIAL_OIT_ENABLED)
import bevy_core_pipeline::oit::draw::oit_draw;
@if(FORWARD_DECAL)
import bevy_pbr::decal::forward::get_forward_decal_info;

// ============================================================================
// BEGIN CUSTOM PATCH: [PointCloudPlugin] - Point size and depth attenuation
// `VertexOutput` is no more imported in each case because imported below.
// ============================================================================

// ============================================================================
// END CUSTOM PATCH: [PointCloudPlugin]
// ============================================================================

@fragment
fn fragment(
    @if(MESHLET_MESH_MATERIAL_PASS)
    @builtin(position) frag_coord: vec4<f32>,
    @if(!MESHLET_MESH_MATERIAL_PASS)
    vertex_output: VertexOutput,
    @if(!MESHLET_MESH_MATERIAL_PASS)
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    @if(MESHLET_MESH_MATERIAL_PASS)
    let vertex_output = resolve_vertex_output(frag_coord);
    @if(MESHLET_MESH_MATERIAL_PASS)
    let is_front = true;

    if pointcloud_functions::dollhouse_should_discard(vertex_output.world_position.xyz) {
        discard;
    }

// ============================================================================
// BEGIN CUSTOM PATCH: [PointCloudPlugin] - Point size and depth attenuation
// ============================================================================

        // Perfect circle
        @if(SHAPE_UVS_A && SPLAT_RADIUS)
        let dist = distance(vertex_output.shape_uv, vec2<f32>(0.5, 0.5));

        @if(SHAPE_UVS_A && SPLAT_RADIUS)
        if dist > pointcloud.radius {
            discard;
        }

    var in: PbrVertexOutput;

    in.position                    = vertex_output.position;
    in.world_position              = vertex_output.world_position;
    in.world_normal                = vertex_output.world_normal;

        @if(VERTEX_UVS_A)
        in.uv                      = vertex_output.uv;
        @if(VERTEX_UVS_B)
        in.uv_b                    = vertex_output.uv_b;
        @if(VERTEX_TANGENTS)
        in.world_tangent           = vertex_output.world_tangent;
        @if(VERTEX_COLORS)
        in.color                   = vec4<f32>(
            functions::srgb_to_rgb_simple(vertex_output.color.xyz),
            vertex_output.color.a,
        );
        @if(VERTEX_OUTPUT_INSTANCE_INDEX)
        in.instance_index          = vertex_output.instance_index;
        @if(VISIBILITY_RANGE_DITHER)
        in.visibility_range_dither = vertex_output.visibility_range_dither;

// ============================================================================
// END CUSTOM PATCH: [PointCloudPlugin]
// ============================================================================

    // If we're in the crossfade section of a visibility range, conditionally
    // discard the fragment according to the visibility pattern.
    @if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);

    @if(FORWARD_DECAL)
    let forward_decal_info = get_forward_decal_info(in);
    @if(FORWARD_DECAL)
    in.world_position = forward_decal_info.world_position;
    @if(FORWARD_DECAL)
    in.uv = forward_decal_info.uv;

    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);

    // clustered decals
    apply_decals(&pbr_input);

    // write the gbuffer, lighting pass id, and optionally normal and motion_vector textures
    @if(PREPASS_PIPELINE)
    let out = deferred_output(in, pbr_input);
    // in forward mode, we calculate the lit color immediately, and then apply some post-lighting effects here.
    // in deferred mode the lit color and these effects will be calculated in the deferred lighting shader
    @if(!PREPASS_PIPELINE)
    var out: FragmentOutput;
    @if(!PREPASS_PIPELINE && POINTCLOUD_UNLIT)
    out.color = pbr_input.material.base_color;
    @if(!PREPASS_PIPELINE && !POINTCLOUD_UNLIT)
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }

    // apply in-shader post processing (fog, alpha-premultiply, and also tonemapping, debanding if the camera is non-hdr)
    // note this does not include fullscreen postprocessing effects like bloom.
    @if(!PREPASS_PIPELINE)
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

    @if(MATERIAL_OIT_ENABLED)
    let alpha_mode = pbr_input.material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    @if(MATERIAL_OIT_ENABLED)
    if alpha_mode != pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE {
        // The fragments will only be drawn during the oit resolve pass.
        oit_draw(in.position, out.color);
        discard;
    }

        @if(FORWARD_DECAL)
        out.color.a = min(forward_decal_info.alpha, out.color.a);

    // if ((pbr_bindings::material.flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {
    //     let texture_color = textureSample(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, in.uv);
    //     #ifdef VERTEX_UVS
    //     out.color = texture_color;
    //     #endif
    // }

    // out.color = pbr_bindings::material.base_color;
    // out.color = vec4<f32>(in.uv.x, in.uv.y, in.uv.x, 1.0);

    return out;
}
