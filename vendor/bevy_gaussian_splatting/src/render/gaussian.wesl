import bevy_gaussian_splatting::render::bindings::view;
import bevy_gaussian_splatting::render::bindings::gaussian_uniforms;
import bevy_gaussian_splatting::render::bindings::Entry;
import bevy_gaussian_splatting::material::classification::class_to_rgb;
@if(OUTPUT_SRGB)
    import bevy_gaussian_splatting::material::spherical_harmonics::linear_to_srgb;
import bevy_gaussian_splatting::material::depth::depth_to_rgb;
import bevy_gaussian_splatting::material::optical_flow::calculate_motion_vector;
import bevy_gaussian_splatting::material::optical_flow::optical_flow_to_rgb;
import bevy_gaussian_splatting::render::helpers::get_rotation_matrix;
import bevy_gaussian_splatting::render::helpers::get_scale_matrix;
import bevy_gaussian_splatting::render::transform::world_to_clip;
import bevy_gaussian_splatting::render::transform::in_frustum;
import bevy_gaussian_splatting::render::transform::splat_center_is_projectable;
import bevy_gaussian_splatting::render::transform::project_splat_center;
import bevy_gaussian_splatting::render::transform::world_view_direction;
@if(!SOG_PROJECTED && GAUSSIAN_2D)
    import bevy_gaussian_splatting::render::gaussian_2d::compute_cov2d_surfel;
@if(!SOG_PROJECTED && GAUSSIAN_2D)
import bevy_gaussian_splatting::render::gaussian_2d::get_bounding_box_cov2d;
@if(!SOG_PROJECTED && GAUSSIAN_2D)
import bevy_gaussian_splatting::render::gaussian_2d::surfel_fragment_power;
@if(!SOG_PROJECTED && (!GAUSSIAN_2D && GAUSSIAN_3D))
    import bevy_gaussian_splatting::render::gaussian_3d::compute_cov2d_3dgs;
@if(!SOG_PROJECTED && (!GAUSSIAN_2D && GAUSSIAN_3D))
    import bevy_gaussian_splatting::render::helpers::get_bounding_box_clip;
@if(!SOG_PROJECTED && (!GAUSSIAN_2D && GAUSSIAN_3D))
import bevy_gaussian_splatting::render::helpers::bound_kernel_covariance;
@if(!SOG_PROJECTED && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
    import bevy_gaussian_splatting::render::gaussian_4d::conditional_cov3d;
@if(!SOG_PROJECTED && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
    import bevy_gaussian_splatting::render::helpers::cov2d;
@if(!SOG_PROJECTED && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
import bevy_gaussian_splatting::render::helpers::get_bounding_box_clip;
@if(!SOG_PROJECTED && PACKED && PRECOMPUTE_COVARIANCE_3D)
        import bevy_gaussian_splatting::render::packed::get_position;
@if(!SOG_PROJECTED && PACKED && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_color;
@if(!SOG_PROJECTED && PACKED && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_visibility;
@if(!SOG_PROJECTED && PACKED && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_opacity;
@if(!SOG_PROJECTED && PACKED && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_cov3d;
@if(!SOG_PROJECTED && PACKED && !PRECOMPUTE_COVARIANCE_3D)
        import bevy_gaussian_splatting::render::packed::get_position;
@if(!SOG_PROJECTED && PACKED && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_color;
@if(!SOG_PROJECTED && PACKED && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_visibility;
@if(!SOG_PROJECTED && PACKED && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_opacity;
@if(!SOG_PROJECTED && PACKED && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_rotation;
@if(!SOG_PROJECTED && PACKED && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::packed::get_scale;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && PRECOMPUTE_COVARIANCE_3D)
        import bevy_gaussian_splatting::render::planar::get_position;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_color;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_visibility;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_opacity;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_cov3d;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && !PRECOMPUTE_COVARIANCE_3D)
        import bevy_gaussian_splatting::render::planar::get_position;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_color;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_visibility;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_opacity;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_rotation;
@if(!SOG_PROJECTED && (!PACKED && BUFFER_STORAGE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::planar::get_scale;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && PRECOMPUTE_COVARIANCE_3D)
        import bevy_gaussian_splatting::render::texture::get_position;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_color;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_visibility;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_opacity;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_cov3d;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::location;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
        import bevy_gaussian_splatting::render::texture::get_position;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_color;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_visibility;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_opacity;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_rotation;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::get_scale;
@if(!SOG_PROJECTED && (!(PACKED || BUFFER_STORAGE) && BUFFER_TEXTURE) && !PRECOMPUTE_COVARIANCE_3D)
import bevy_gaussian_splatting::render::texture::location;
@if(SOG_PROJECTED)
import bevy_gaussian_splatting::render::bindings::projected_records;

@if(STOCHASTIC)
    fn get_entry(index: u32) -> Entry {
        return Entry(0u, index);
    }
    @if(!STOCHASTIC && BUFFER_STORAGE)
    @group(3) @binding(0) var<storage, read> sorted_entries: array<Entry>;
    @if(!STOCHASTIC && BUFFER_STORAGE)
    fn get_entry(index: u32) -> Entry {
        return sorted_entries[index];
    }
    @if(!STOCHASTIC && (!BUFFER_STORAGE && BUFFER_TEXTURE))
    @group(3) @binding(0) var sorted_entries: texture_2d<u32>;
    @if(!STOCHASTIC && (!BUFFER_STORAGE && BUFFER_TEXTURE))
    fn get_entry(index: u32) -> Entry {
        let sample = textureLoad(
            sorted_entries,
            location(index),
            0,
        );

        return Entry(
            sample.r,
            sample.g,
        );
    }

    @if(WEBGL2)
    struct GaussianVertexOutput {
        @builtin(position) position: vec4<f32>,
        @location(0) color: vec4<f32>,
        @location(1) uv: vec2<f32>,
        @if(WEBGL2 && GAUSSIAN_2D)
        @location(2) local_to_pixel_u: vec3<f32>,
        @if(WEBGL2 && GAUSSIAN_2D)
        @location(3) local_to_pixel_v: vec3<f32>,
        @if(WEBGL2 && GAUSSIAN_2D)
        @location(4) local_to_pixel_w: vec3<f32>,
        @if(WEBGL2 && GAUSSIAN_2D)
        @location(5) mean_2d: vec2<f32>,
        @if(WEBGL2 && GAUSSIAN_2D)
        @location(6) radius: vec2<f32>,
        @if(WEBGL2 && (!GAUSSIAN_2D && GAUSSIAN_3D))
        @location(2) conic: vec3<f32>,
        @if(WEBGL2 && (!GAUSSIAN_2D && GAUSSIAN_3D))
        @location(3) major_minor: vec2<f32>,
        @if(WEBGL2 && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
        @location(2) conic: vec3<f32>,
        @if(WEBGL2 && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
        @location(3) major_minor: vec2<f32>,
    };
    @if(!WEBGL2)
    struct GaussianVertexOutput {
        @builtin(position) position: vec4<f32>,
        @location(0) @interpolate(flat) color: vec4<f32>,
        @location(1) @interpolate(linear) uv: vec2<f32>,
        @if(!WEBGL2 && STOCHASTIC)
        @location(7) @interpolate(flat) splat_id: u32,
        @if(!WEBGL2 && GAUSSIAN_2D)
        @location(2) @interpolate(flat) local_to_pixel_u: vec3<f32>,
        @if(!WEBGL2 && GAUSSIAN_2D)
        @location(3) @interpolate(flat) local_to_pixel_v: vec3<f32>,
        @if(!WEBGL2 && GAUSSIAN_2D)
        @location(4) @interpolate(flat) local_to_pixel_w: vec3<f32>,
        @if(!WEBGL2 && GAUSSIAN_2D)
        @location(5) @interpolate(flat) mean_2d: vec2<f32>,
        @if(!WEBGL2 && GAUSSIAN_2D)
        @location(6) @interpolate(flat) radius: vec2<f32>,
        @if(!WEBGL2 && (!GAUSSIAN_2D && GAUSSIAN_3D))
        @location(2) @interpolate(flat) conic: vec3<f32>,
        @if(!WEBGL2 && (!GAUSSIAN_2D && GAUSSIAN_3D))
        @location(3) @interpolate(linear) major_minor: vec2<f32>,
        @if(!WEBGL2 && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
        @location(2) @interpolate(flat) conic: vec3<f32>,
        @if(!WEBGL2 && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
        @location(3) @interpolate(linear) major_minor: vec2<f32>,
    };

// Shared by the production vertex/fragment paths and headless numerical tests.
fn raster_cutoff(opacity: f32) -> f32 {
    @if(NORMALIZED_GAUSSIAN)
    return sqrt(8.0);
        @if(!NORMALIZED_GAUSSIAN && OPACITY_ADAPTIVE_RADIUS)
        return sqrt(max(9.0 + 2.0 * log(opacity), 0.000001));
        @if(!NORMALIZED_GAUSSIAN && !OPACITY_ADAPTIVE_RADIUS)
        return 3.0;
}

fn raster_obb_power(uv: vec2<f32>) -> f32 {
    @if(NORMALIZED_GAUSSIAN)
    return -4.0 * dot(uv, uv);
    @if(!NORMALIZED_GAUSSIAN)
    let sigma = 1.0 / 3.0;
    @if(!NORMALIZED_GAUSSIAN)
    let sigma_squared = 2.0 * sigma * sigma;
    @if(!NORMALIZED_GAUSSIAN)
    return -dot(uv, uv) / sigma_squared;
}

fn raster_aabb_power(d: vec2<f32>, conic: vec3<f32>) -> f32 {
    @if(NORMALIZED_GAUSSIAN)
    return -0.5 * (conic.x * d.x * d.x + conic.z * d.y * d.y) - conic.y * d.x * d.y;
    @if(!NORMALIZED_GAUSSIAN)
    return -0.5 * (conic.x * d.x * d.x + conic.z * d.y * d.y) + conic.y * d.x * d.y;
}

fn raster_alpha(power: f32, opacity: f32) -> f32 {
    @if(NORMALIZED_GAUSSIAN)
    let edge = exp(-4.0);
    @if(NORMALIZED_GAUSSIAN)
    return max(0.0, (exp(power) - edge) / (1.0 - edge)) * opacity;
    @if(!NORMALIZED_GAUSSIAN)
    return min(exp(power) * opacity, 0.999);
}

fn raster_is_clipped(power: f32, alpha: f32) -> bool {
    @if(NORMALIZED_GAUSSIAN)
    return power < -4.0 || alpha < 1.0 / 255.0;
    @if(!NORMALIZED_GAUSSIAN)
    return false;
}

fn world_to_local_direction(ray_direction_world: vec3<f32>, transform: mat4x4<f32>) -> vec3<f32> {
    let basis = mat3x3<f32>(
        transform[0].xyz,
        transform[1].xyz,
        transform[2].xyz,
    );
    let basis_x = normalize(basis[0]);
    let basis_y = normalize(basis[1]);
    let basis_z = normalize(basis[2]);

    let local = vec3<f32>(
        dot(basis_x, ray_direction_world),
        dot(basis_y, ray_direction_world),
        dot(basis_z, ray_direction_world),
    );

    return normalize(local);
}
@if(SOG_PROJECTED)
@vertex
fn vs_points(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
) -> GaussianVertexOutput {
    var output: GaussianVertexOutput;
    let entry = get_entry(instance_index);
    @if(SOG_PROJECTED && STOCHASTIC)
    output.splat_id = instance_index;
    let record = projected_records[entry.value];
    if record.centre_key.w == 0xFFFFFFFFu {
        output.position = vec4<f32>(0.0);
        return output;
    }
    let corners = array<vec2<f32>, 4>(vec2<f32>(-1.0, -1.0), vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0));
    let uv = corners[vertex_index % 4u];
    let x = unpack2x16float(record.axes_colour.x);
    let y = unpack2x16float(record.axes_colour.y);
    let centre = bitcast<vec3<f32>>(record.centre_key.xyz);
    output.position = vec4<f32>(centre.xy + x * uv.x + y * uv.y, centre.z, 1.0);
    output.color = vec4<f32>(unpack2x16float(record.axes_colour.z), unpack2x16float(record.axes_colour.w));
    output.uv = uv;
    return output;
}
@if(!SOG_PROJECTED)
@vertex
fn vs_points(
    @builtin(instance_index) instance_index: u32,
    @builtin(vertex_index) vertex_index: u32,
) -> GaussianVertexOutput {
    var output: GaussianVertexOutput;

    let entry = get_entry(instance_index);
    @if(!SOG_PROJECTED && STOCHASTIC)
    output.splat_id = instance_index;
    let splat_index = entry.value;

    var discard_quad = false;

    discard_quad |= entry.key == 0xFFFFFFFFu; // || splat_index == 0u;

    let position = vec4<f32>(get_position(splat_index), 1.0);

    var transformed_position = (gaussian_uniforms.transform * position).xyz;
    var previous_transformed_position = transformed_position;

    @if(!SOG_PROJECTED && DRAW_SELECTED)
    discard_quad |= get_visibility(splat_index) < 0.5;

    @if(!SOG_PROJECTED && GAUSSIAN_3D)
    discard_quad |= !splat_center_is_projectable(transformed_position);
    @if(!SOG_PROJECTED && (!(GAUSSIAN_3D || GAUSSIAN_4D)))
    let projected_position = world_to_clip(transformed_position);
    @if(!SOG_PROJECTED && (!(GAUSSIAN_3D || GAUSSIAN_4D)))
    discard_quad |= !in_frustum(projected_position.xyz);

    if (discard_quad) {
        output.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        output.position = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return output;
    }

    // Hardware XY clipping operates on the projected quad, including off-screen
    // centres whose covariance footprint overlaps the viewport. Do not reinstate
    // point-frustum rejection here or in position-only radix/compaction stages.
    @if(!SOG_PROJECTED && GAUSSIAN_3D)
    let projected_position = project_splat_center(transformed_position);

    var quad_vertices = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    let quad_index = vertex_index % 4u;
    let quad_offset = quad_vertices[quad_index];

    var opacity = get_opacity(splat_index);

    let cutoff = raster_cutoff(opacity);

    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    let surfel = compute_cov2d_surfel(
        transformed_position,
        splat_index,
        cutoff,
    );

    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    output.local_to_pixel_u = surfel.local_to_pixel[0];
    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    output.local_to_pixel_v = surfel.local_to_pixel[1];
    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    output.local_to_pixel_w = surfel.local_to_pixel[2];
    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    output.mean_2d = surfel.mean_2d;

    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    let bb = get_bounding_box_cov2d(
        surfel.extent,
        quad_offset,
        cutoff,
    );
    @if(!SOG_PROJECTED && GAUSSIAN_2D)
    output.radius = bb.zw;
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && GAUSSIAN_3D)
        let gaussian_cov2d = bound_kernel_covariance(compute_cov2d_3dgs(
            transformed_position,
            splat_index,
        ), cutoff);
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        let gaussian_4d = conditional_cov3d(
            transformed_position,
            splat_index,
            gaussian_uniforms.time,
        );

        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        if !gaussian_4d.mask {
            output.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            output.position = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            return output;
        }

        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        let position_t = vec4<f32>(position.xyz + gaussian_4d.delta_mean, 1.0);
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        transformed_position = (gaussian_uniforms.transform * position_t).xyz;
        // TODO: set previous_transformed_position based on temporal position delta
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        let projected_position = world_to_clip(transformed_position);

        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        if !in_frustum(projected_position.xyz) {
            output.color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            output.position = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            return output;
        }

        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        opacity = opacity * gaussian_4d.opacity_modifier;

        @if(!SOG_PROJECTED && !GAUSSIAN_2D && (!GAUSSIAN_3D && GAUSSIAN_4D))
        let gaussian_cov2d = cov2d(
            transformed_position,
            gaussian_4d.cov3d,
        );

    @if(!SOG_PROJECTED && !GAUSSIAN_2D)
    let bb = get_bounding_box_clip(
        gaussian_cov2d,
        quad_offset,
        cutoff,
    );

        @if(!SOG_PROJECTED && !GAUSSIAN_2D && USE_AABB)
        let det = gaussian_cov2d.x * gaussian_cov2d.z - gaussian_cov2d.y * gaussian_cov2d.y;
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && USE_AABB)
        let det_inv = 1.0 / det;
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && USE_AABB)
        let conic = vec3<f32>(
            gaussian_cov2d.z * det_inv,
            -gaussian_cov2d.y * det_inv,
            gaussian_cov2d.x * det_inv
        );
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && USE_AABB)
        output.conic = conic;
        @if(!SOG_PROJECTED && !GAUSSIAN_2D && USE_AABB)
        output.major_minor = bb.zw;

    var rgb = vec3<f32>(0.0);

// TODO: RASTERIZE_ACCELERATION
    @if(!SOG_PROJECTED && RASTERIZE_CLASSIFICATION)
    let ray_direction_world = world_view_direction(transformed_position);
    @if(!SOG_PROJECTED && RASTERIZE_CLASSIFICATION)
    let ray_direction_local = world_to_local_direction(ray_direction_world, gaussian_uniforms.transform);

        @if(!SOG_PROJECTED && RASTERIZE_CLASSIFICATION && GAUSSIAN_3D_STRUCTURE)
        rgb = get_color(splat_index, ray_direction_local);
        @if(!SOG_PROJECTED && RASTERIZE_CLASSIFICATION && (!GAUSSIAN_3D_STRUCTURE && GAUSSIAN_4D))
        rgb = get_color(splat_index, gaussian_4d.dir_t, ray_direction_local);

    @if(!SOG_PROJECTED && RASTERIZE_CLASSIFICATION)
    rgb = class_to_rgb(
        get_visibility(splat_index),
        rgb,
    );
    // TODO: unbiased depth rendering, see: https://zju3dv.github.io/pgsr/
    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let first_position = vec4<f32>(get_position(get_entry(1u).value), 1.0);
    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let last_position = vec4<f32>(get_position(get_entry(gaussian_uniforms.count - 1u).value), 1.0);

    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let min_position = (gaussian_uniforms.transform * last_position).xyz;
    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let max_position = (gaussian_uniforms.transform * first_position).xyz;

    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let camera_position = view.world_position;

    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let min_distance = length(min_position - camera_position);
    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let max_distance = length(max_position - camera_position);

    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    let depth = length(transformed_position - camera_position);
    @if(!SOG_PROJECTED && (!RASTERIZE_CLASSIFICATION && RASTERIZE_DEPTH))
    rgb = depth_to_rgb(
        depth,
        min_distance,
        max_distance,
    );
    // TODO: support rotation decomposition for 4d gaussians
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let R = get_rotation_matrix(get_rotation(splat_index));
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let S = get_scale_matrix(get_scale(splat_index));
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let T = mat3x3<f32>(
        gaussian_uniforms.transform[0].xyz,
        gaussian_uniforms.transform[1].xyz,
        gaussian_uniforms.transform[2].xyz,
    );
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let L = T * S * R;

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let local_normal = vec4<f32>(L[2], 0.0);
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let world_normal = view.view_from_world * local_normal;

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    let t = normalize(world_normal);

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH) && RASTERIZE_NORMAL))
    rgb = vec3<f32>(
        0.5 * (t.x + 1.0),
        0.5 * (t.y + 1.0),
        0.5 * (t.z + 1.0)
    );
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL) && RASTERIZE_OPTICAL_FLOW))
    let motion_vector = calculate_motion_vector(
        transformed_position,
        previous_transformed_position,
    );

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL) && RASTERIZE_OPTICAL_FLOW))
    rgb = optical_flow_to_rgb(motion_vector);
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW) && RASTERIZE_POSITION))
    rgb = (transformed_position - gaussian_uniforms.min.xyz) / (gaussian_uniforms.max.xyz - gaussian_uniforms.min.xyz);
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let time_delta = 1e-3;
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let future_gaussian_4d = conditional_cov3d(
        transformed_position,
        splat_index,
        gaussian_uniforms.time + time_delta,
    );
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let position_delta = future_gaussian_4d.delta_mean - gaussian_4d.delta_mean;
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let velocity = position_delta / time_delta;
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let velocity_magnitude = length(velocity);
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let velocity_normalized = normalize(velocity);

    // TODO: magnitude normalization
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let min_magnitude = 1.0;
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let max_magnitude = 2.0;

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let scaled_mag = clamp(
        (velocity_magnitude - min_magnitude) / (max_magnitude - min_magnitude),
        0.0,
        1.0
    );

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    if scaled_mag < 1e-2 {
        opacity = 0.0;
    }

    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    let base_color = 0.5 * (velocity_normalized + vec3<f32>(1.0, 1.0, 1.0));
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION) && RASTERIZE_VELOCITY))
    rgb = base_color * scaled_mag;
    // TODO: verify color benefit for ray_direction computed at quad verticies instead of gaussian center (same as current complexity)
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION || RASTERIZE_VELOCITY) && RASTERIZE_COLOR))
    let ray_direction_world = world_view_direction(transformed_position);
    @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION || RASTERIZE_VELOCITY) && RASTERIZE_COLOR))
    let ray_direction_local = world_to_local_direction(ray_direction_world, gaussian_uniforms.transform);

        @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION || RASTERIZE_VELOCITY) && RASTERIZE_COLOR) && GAUSSIAN_3D_STRUCTURE)
        rgb = get_color(splat_index, ray_direction_local);
        @if(!SOG_PROJECTED && (!(RASTERIZE_CLASSIFICATION || RASTERIZE_DEPTH || RASTERIZE_NORMAL || RASTERIZE_OPTICAL_FLOW || RASTERIZE_POSITION || RASTERIZE_VELOCITY) && RASTERIZE_COLOR) && (!GAUSSIAN_3D_STRUCTURE && GAUSSIAN_4D))
        rgb = get_color(splat_index, gaussian_4d.dir_t, ray_direction_local);

    output.color = vec4<f32>(
        rgb,
        opacity * gaussian_uniforms.global_opacity,
    );

    @if(!SOG_PROJECTED && HIGHLIGHT_SELECTED)
    if (get_visibility(splat_index) > 0.5) {
        output.color = vec4<f32>(0.3, 1.0, 0.1, 1.0);
    }

    output.uv = quad_offset;
    output.position = vec4<f32>(
        projected_position.xy + bb.xy,
        projected_position.zw,
    );

    return output;
}

@fragment
fn fs_main(input: GaussianVertexOutput) -> @location(0) vec4<f32> {
    @if(USE_AABB && GAUSSIAN_2D)
    let radius = input.radius;
    @if(USE_AABB && GAUSSIAN_2D)
    let mean_2d = input.mean_2d;
    @if(USE_AABB && GAUSSIAN_2D)
    let aspect = vec2<f32>(
        1.0,
        view.viewport.z / view.viewport.w,
    );
    @if(USE_AABB && GAUSSIAN_2D)
    let pixel_coord = input.uv * radius * aspect + mean_2d;

    @if(USE_AABB && GAUSSIAN_2D)
    let power = surfel_fragment_power(
        mat3x3<f32>(
            input.local_to_pixel_u,
            input.local_to_pixel_v,
            input.local_to_pixel_w,
        ),
        pixel_coord,
        mean_2d,
    );
    @if(USE_AABB && (!GAUSSIAN_2D && GAUSSIAN_3D))
    let d = -input.major_minor;
    @if(USE_AABB && (!GAUSSIAN_2D && GAUSSIAN_3D))
    let conic = input.conic;
    @if(USE_AABB && (!GAUSSIAN_2D && GAUSSIAN_3D))
    let power = raster_aabb_power(d, conic);
    @if(USE_AABB && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
    let d = -input.major_minor;
    @if(USE_AABB && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
    let conic = input.conic;
    @if(USE_AABB && (!(GAUSSIAN_2D || GAUSSIAN_3D) && GAUSSIAN_4D))
    let power = -0.5 * (conic.x * d.x * d.x + conic.z * d.y * d.y) + conic.y * d.x * d.y;

    @if(USE_AABB)
    if (power > 0.0) {
        discard;
    }

    @if(USE_OBB)
    let distance_squared = dot(input.uv, input.uv);

    @if(USE_OBB)
    let power = raster_obb_power(input.uv);

    @if(USE_OBB)
    if (distance_squared > 3.0 * 3.0) {
        discard;
    }

    @if(VISUALIZE_BOUNDING_BOX)
    let uv = input.uv * 0.5 + 0.5;
    @if(VISUALIZE_BOUNDING_BOX)
    let edge_width = 0.08;
    @if(VISUALIZE_BOUNDING_BOX)
    if (
        (uv.x < edge_width || uv.x > 1.0 - edge_width) ||
        (uv.y < edge_width || uv.y > 1.0 - edge_width)
    ) {
        return fragment_output(vec3<f32>(0.3, 1.0, 0.1), 1.0);
    }

    let alpha = raster_alpha(power, input.color.a);

    // The fixed sqrt(8) support corresponds to power=-4 in either geometry path.
    // Forward alpha clipping matches the pinned reference's 1/255 threshold.
    if (raster_is_clipped(power, alpha)) {
        discard;
    }

    // Picking depth only (colour writes are masked): keep visibly opaque splat
    // footprints and drop faint fringes.
    @if(DEPTH_PASS)
    if alpha < bitcast<f32>(u32(constants::DEPTH_PASS_ALPHA_BITS)) {
        discard;
    }
    @if(DEPTH_PASS)
    return vec4<f32>(0.0);

    // Independent per-(pixel, cloud, source record, explicit sample) coverage.
    // Source remapping can change noise: no temporal history is promised.
    @if(STOCHASTIC)
    let pixel = vec2<u32>(input.position.xy);
    @if(STOCHASTIC)
    let seed = stochastic_hash(gaussian_uniforms.stochastic_seed ^ input.splat_id);
    @if(STOCHASTIC)
    let sample = stochastic_hash(gaussian_uniforms.stochastic_sample ^ seed);
    @if(STOCHASTIC)
    let bits = stochastic_hash(pixel.x ^ stochastic_hash(pixel.y ^ sample));
    @if(STOCHASTIC)
    let threshold = f32(bits >> 8u) / 16777216.0;
    @if(STOCHASTIC)
    if threshold >= clamp(alpha, 0.0, 1.0) { discard; }
    @if(STOCHASTIC)
    return fragment_output(input.color.rgb, 1.0);

    return fragment_output(input.color.rgb, alpha);
}

fn fragment_output(color: vec3<f32>, alpha: f32) -> vec4<f32> {
    // Input SH is still interpreted normally. Encoding must happen before
    // premultiplication, and only into a deliberately encoded target.
    @if(OUTPUT_SRGB)
    return vec4<f32>(linear_to_srgb(color) * alpha, alpha);
    @if(!OUTPUT_SRGB)
    return vec4<f32>(color * alpha, alpha);
}

// Integer hashing avoids backend-dependent sin/dot pseudo-random generators.
fn stochastic_hash(value: u32) -> u32 {
    var x = value;
    x = (x ^ (x >> 16u)) * 0x7feb352du;
    x = (x ^ (x >> 15u)) * 0x846ca68bu;
    return x ^ (x >> 16u);
}
