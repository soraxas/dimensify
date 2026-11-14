import bevy_pbr::render::view_transformations::position_world_to_view;
import bevy_pbr::render::forward_io::FragmentOutput;
import bevy_pointcloud::assets::shaders::forward_io::ShapeInput;
import bevy_pointcloud::assets::shaders::forward_io::InstanceInput;
import bevy_pointcloud::assets::shaders::forward_io::VertexOutput;
import bevy_pointcloud::assets::shaders::functions;
import bevy_pointcloud::assets::shaders::pointcloud_bindings::pointcloud;
import bevy_pointcloud::assets::shaders::pointcloud_functions;
import bevy_pointcloud::materials::simple::simple_types as material_types;
import bevy_pointcloud::materials::simple::simple_functions as material_functions;
import bevy_pointcloud::materials::simple::simple_bindings as material_bindings;

@fragment
fn fragment(
    vertex_output: VertexOutput,
) -> FragmentOutput {
    if pointcloud_functions::dollhouse_should_discard(vertex_output.world_position.xyz) {
        discard;
    }
    var in = vertex_output;
    var out: FragmentOutput;

        // Perfect circle
        @if(SHAPE_UVS_A && SPLAT_RADIUS)
        let dist = distance(in.shape_uv, vec2<f32>(0.5, 0.5));

        @if(SHAPE_UVS_A && SPLAT_RADIUS)
        if dist > pointcloud.radius {
            discard;
        }

    var base_color = material_bindings::material.base_color;

        @if(SIMPLE_MATERIAL_GRADIENT)
        let projected_dist = dot(in.world_position.xyz, material_bindings::material.gradient_direction.xyz);
        @if(SIMPLE_MATERIAL_GRADIENT)
        let range = material_bindings::material.gradient_end - material_bindings::material.gradient_start;
        @if(SIMPLE_MATERIAL_GRADIENT)
        var t = 0.0;
        @if(SIMPLE_MATERIAL_GRADIENT)
        if range > 0.0 {
            t = clamp((projected_dist - material_bindings::material.gradient_start) / range, 0.0, 1.0);
        }

        @if(SIMPLE_MATERIAL_GRADIENT)
        base_color = material_functions::evaluate_gradient(
            material_bindings::material.base_color,
            material_bindings::material.end_color,
            material_bindings::material.color_stops,
        t);

    if ((material_bindings::material.flags & material_types::SIMPLE_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {

        // We determine which UV coordinates to use for the texture sampling
        var texture_uv = in.uv; //vec2<f32>(0.5, 0.5);

        // Apply 2D affine transformation on global/planar UV coordinates
            @if(SIMPLE_MATERIAL_HAS_UV_TRANSFORM)
            let uv_rot_scale = mat2x2<f32>(
                material_bindings::material.uv_transform_a.xy,
                material_bindings::material.uv_transform_a.zw
            );
            @if(SIMPLE_MATERIAL_HAS_UV_TRANSFORM)
            let uv_translation = material_bindings::material.uv_transform_b.xy;

            @if(SIMPLE_MATERIAL_HAS_UV_TRANSFORM)
            texture_uv = (uv_rot_scale * texture_uv) + uv_translation;

        // Example of texture sampling (adapt with your actual binding names)
        let texture_color = textureSample(material_bindings::base_color_texture, material_bindings::base_color_sampler, texture_uv);

        base_color *= texture_color;
    }

        @if(VERTEX_COLORS)
        out.color = vec4<f32>(functions::srgb_to_rgb_simple(in.color.xyz * base_color.xyz), 1.0);
        @if(!VERTEX_COLORS)
        out.color = vec4<f32>(functions::srgb_to_rgb_simple(base_color.xyz), 1.0);

    return out;
}
