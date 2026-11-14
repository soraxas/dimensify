import bevy_pbr::render::pbr_prepass_functions;
import bevy_pbr::render::pbr_bindings;
import bevy_pbr::render::pbr_bindings::material;
import bevy_pbr::render::pbr_types;
import bevy_pbr::render::pbr_functions;
import bevy_pbr::render::pbr_functions::SampleBias;
import bevy_pbr::prepass::io as prepass_io;
import bevy_pbr::render::mesh_view_bindings::view;
import bevy_render::bindless::bindless_samplers_filtering;
import bevy_render::bindless::bindless_textures_2d;
import bevy_pbr::prepass::io::VertexOutput as PbrVertexOutput;
import bevy_pbr::prepass::io::FragmentOutput;
import bevy_pointcloud::render::prepass::prepass_io::VertexOutput;
import bevy_pointcloud::assets::shaders::pointcloud_bindings::pointcloud;
import bevy_pointcloud::assets::shaders::pointcloud_functions;

// ============================================================================
// BEGIN CUSTOM PATCH: [PointCloudPlugin] - Point size and depth attenuation
// `VertexOutput` is no more imported in each case because imported below.
// ============================================================================

// ============================================================================
// END CUSTOM PATCH: [PointCloudPlugin]
// ============================================================================

@if(PREPASS_FRAGMENT)
@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
)
-> prepass_io::FragmentOutput
{
    if pointcloud_functions::dollhouse_should_discard(vertex_output.world_position.xyz) {
        discard;
    }
        // Perfect circle
        @if(PREPASS_FRAGMENT && SHAPE_UVS_A && SPLAT_RADIUS)
        let dist = distance(vertex_output.shape_uv, vec2<f32>(0.5, 0.5));

        @if(PREPASS_FRAGMENT && SHAPE_UVS_A && SPLAT_RADIUS)
        if dist > pointcloud.radius {
            discard;
        }

    var in: PbrVertexOutput;

    in.position                    = vertex_output.position;

        @if(PREPASS_FRAGMENT && VERTEX_UVS_A)
        in.uv                      = vertex_output.uv;

        @if(PREPASS_FRAGMENT && VERTEX_UVS_B)
        in.uv_b                    = vertex_output.uv_b;

        @if(PREPASS_FRAGMENT && NORMAL_PREPASS_OR_DEFERRED_PREPASS)
        in.world_normal            = vertex_output.world_normal;

            @if(PREPASS_FRAGMENT && NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
            in.world_tangent       = vertex_output.world_tangent;

    in.world_position              = vertex_output.world_position;
        @if(PREPASS_FRAGMENT && MOTION_VECTOR_PREPASS)
        in.previous_world_position = vertex_output.previous_world_position;

        @if(PREPASS_FRAGMENT && UNCLIPPED_DEPTH_ORTHO_EMULATION)
        in.unclipped_depth         = vertex_output.unclipped_depth;

        @if(PREPASS_FRAGMENT && VERTEX_OUTPUT_INSTANCE_INDEX)
        in.instance_index          = vertex_output.instance_index;

        @if(PREPASS_FRAGMENT && VERTEX_COLORS)
        in.color                   = vertex_output.color;

        @if(PREPASS_FRAGMENT && VISIBILITY_RANGE_DITHER)
        in.visibility_range_dither = vertex_output.visibility_range_dither;

        @if(PREPASS_FRAGMENT && UNCLIPPED_DEPTH_ORTHO_EMULATION)
        in.unclipped_depth = vertex_output.unclipped_depth;

    let flags = pbr_bindings::material.flags;
    let uv_transform = pbr_bindings::material.uv_transform;

    // If we're in the crossfade section of a visibility range, conditionally
    // discard the fragment according to the visibility pattern.
        @if(PREPASS_FRAGMENT && VISIBILITY_RANGE_DITHER)
        visibility_range_dither(in.position, in.visibility_range_dither);

    pbr_prepass_functions::prepass_sample_color_and_alpha_discard(in);

    var out: prepass_io::FragmentOutput;

        @if(PREPASS_FRAGMENT && UNCLIPPED_DEPTH_ORTHO_EMULATION)
        out.frag_depth = in.unclipped_depth;

        // NOTE: Unlit bit not set means == 0 is true, so the true case is if lit
        @if(PREPASS_FRAGMENT && NORMAL_PREPASS)
        if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
            let double_sided = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;

            let world_normal = pbr_functions::prepare_world_normal(
                in.world_normal,
                double_sided,
                is_front,
            );

            var normal = world_normal;

    // TODO: Transforming UVs mean we need to apply derivative chain rule for meshlet mesh material pass
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && STANDARD_MATERIAL_NORMAL_MAP_UV_B)
            let uv = (uv_transform * vec3(in.uv_b, 1.0)).xy;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && !STANDARD_MATERIAL_NORMAL_MAP_UV_B)
            let uv = (uv_transform * vec3(in.uv, 1.0)).xy;

            // Fill in the sample bias so we can sample from textures.
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP)
            var bias: SampleBias;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && MESHLET_MESH_MATERIAL_PASS)
            bias.ddx_uv = in.ddx_uv;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && MESHLET_MESH_MATERIAL_PASS)
            bias.ddy_uv = in.ddy_uv;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && !MESHLET_MESH_MATERIAL_PASS)
            bias.mip_bias = view.mip_bias;

            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && MESHLET_MESH_MATERIAL_PASS && BINDLESS)
            let Nt = textureSampleGrad(
                bindless_textures_2d[material_indices[slot].normal_map_texture],
                bindless_samplers_filtering[material_indices[slot].normal_map_sampler],
                uv,
                bias.ddx_uv, bias.ddy_uv,
            ).rgb;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && MESHLET_MESH_MATERIAL_PASS && !BINDLESS)
            let Nt = textureSampleGrad(
                pbr_bindings::normal_map_texture,
                pbr_bindings::normal_map_sampler,
                uv,
                bias.ddx_uv, bias.ddy_uv,
            ).rgb;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && !MESHLET_MESH_MATERIAL_PASS && BINDLESS)
            let Nt = textureSampleBias(
                bindless_textures_2d[material_indices[slot].normal_map_texture],
                bindless_samplers_filtering[material_indices[slot].normal_map_sampler],
                uv,
                bias.mip_bias,
            ).rgb;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP && !MESHLET_MESH_MATERIAL_PASS && !BINDLESS)
            let Nt = textureSampleBias(
                pbr_bindings::normal_map_texture,
                pbr_bindings::normal_map_sampler,
                uv,
                bias.mip_bias,
            ).rgb;
            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP)
            let TBN = pbr_functions::calculate_tbn_mikktspace(normal, in.world_tangent);

            @if(PREPASS_FRAGMENT && NORMAL_PREPASS && VERTEX_UVS && VERTEX_TANGENTS && STANDARD_MATERIAL_NORMAL_MAP)
            normal = pbr_functions::apply_normal_mapping(
                flags,
                TBN,
                double_sided,
                is_front,
                Nt,
            );

            out.normal = vec4(normal * 0.5 + vec3(0.5), 1.0);
        } else {
            out.normal = vec4(in.world_normal * 0.5 + vec3(0.5), 1.0);
        }

        @if(PREPASS_FRAGMENT && MOTION_VECTOR_PREPASS && MESHLET_MESH_MATERIAL_PASS)
        out.motion_vector = in.motion_vector;
        @if(PREPASS_FRAGMENT && MOTION_VECTOR_PREPASS && !MESHLET_MESH_MATERIAL_PASS)
        out.motion_vector = pbr_prepass_functions::calculate_motion_vector(in.world_position, in.previous_world_position);

    return out;
}
@if(!PREPASS_FRAGMENT)
@fragment
fn fragment(
    vertex_output: VertexOutput,
    @builtin(front_facing) is_front: bool,
)
{
    if pointcloud_functions::dollhouse_should_discard(vertex_output.world_position.xyz) {
        discard;
    }
        // Perfect circle
        @if(!PREPASS_FRAGMENT && SHAPE_UVS_A && SPLAT_RADIUS)
        let dist = distance(vertex_output.shape_uv, vec2<f32>(0.5, 0.5));

        @if(!PREPASS_FRAGMENT && SHAPE_UVS_A && SPLAT_RADIUS)
        if dist > pointcloud.radius {
            discard;
        }

    var in: PbrVertexOutput;

    in.position                    = vertex_output.position;

        @if(!PREPASS_FRAGMENT && VERTEX_UVS_A)
        in.uv                      = vertex_output.uv;

        @if(!PREPASS_FRAGMENT && VERTEX_UVS_B)
        in.uv_b                    = vertex_output.uv_b;

        @if(!PREPASS_FRAGMENT && NORMAL_PREPASS_OR_DEFERRED_PREPASS)
        in.world_normal            = vertex_output.world_normal;

            @if(!PREPASS_FRAGMENT && NORMAL_PREPASS_OR_DEFERRED_PREPASS && VERTEX_TANGENTS)
            in.world_tangent       = vertex_output.world_tangent;

    in.world_position              = vertex_output.world_position;
        @if(!PREPASS_FRAGMENT && MOTION_VECTOR_PREPASS)
        in.previous_world_position = vertex_output.previous_world_position;

        @if(!PREPASS_FRAGMENT && UNCLIPPED_DEPTH_ORTHO_EMULATION)
        in.unclipped_depth         = vertex_output.unclipped_depth;

        @if(!PREPASS_FRAGMENT && VERTEX_OUTPUT_INSTANCE_INDEX)
        in.instance_index          = vertex_output.instance_index;

        @if(!PREPASS_FRAGMENT && VERTEX_COLORS)
        in.color                   = vertex_output.color;

        @if(!PREPASS_FRAGMENT && VISIBILITY_RANGE_DITHER)
        in.visibility_range_dither = vertex_output.visibility_range_dither;

        @if(!PREPASS_FRAGMENT && UNCLIPPED_DEPTH_ORTHO_EMULATION)
        in.unclipped_depth = vertex_output.unclipped_depth;

    pbr_prepass_functions::prepass_sample_color_and_alpha_discard(in);
}
