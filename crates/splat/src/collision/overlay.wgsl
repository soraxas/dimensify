// Voxel collision overlay: per-pixel octree ray march, ported from SuperSplat's
// VoxelDebugOverlay. Rays are built in view space and moved to the grid frame
// with grid_from_view, so the march itself is frame-agnostic. Only the fragment position is read from
// Bevy's fullscreen triangle, so no shader import is needed.

const SOLID_LEAF_MARKER: u32 = 0xFF000000u;
const LEAF_SIZE: i32 = 4;
const MAX_STEPS: u32 = 512u;
// Wireframe edge width in pixels.
const EDGE_PIXELS: f32 = 1.5;
// Edges stay tinted so dense grids keep the solid/free distinction.
const EDGE_ALPHA: f32 = 0.8;
const FILL_ALPHA: f32 = 0.4;

struct Uniforms {
    view_from_clip: mat4x4<f32>,
    grid_from_view: mat4x4<f32>,
    viewport: vec4<f32>,
    eye: vec3<f32>,
    pixel_scale: f32,
    grid_min: vec3<f32>,
    voxel_res: f32,
    voxels: vec3<u32>,
    tree_depth: u32,
    inverted: u32,
    orthographic: u32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> nodes: array<u32>;
@group(0) @binding(2) var<storage, read> leaf_data: array<u32>;

// Look up block (bx, by, bz). Returns (result, skip_level):
//   result 0 = empty, 1 = solid, 2+ = mixed leaf (2 + leaf index);
//   skip_level: octree level of the uniform cell containing the block.
fn query_block(b: vec3<i32>) -> vec2<u32> {
    var node_index = 0u;
    var level = uniforms.tree_depth - 1u;
    loop {
        let node = nodes[node_index];
        if (node == SOLID_LEAF_MARKER) {
            return vec2<u32>(1u, level + 1u);
        }
        let child_mask = (node >> 24u) & 0xFFu;
        if (child_mask == 0u) {
            return vec2<u32>(2u + (node & 0x00FFFFFFu), 0u);
        }
        let bits = (vec3<u32>(b) >> vec3<u32>(level)) & vec3<u32>(1u);
        let octant = (bits.z << 2u) | (bits.y << 1u) | bits.x;
        if ((child_mask & (1u << octant)) == 0u) {
            return vec2<u32>(0u, level);
        }
        node_index = (node & 0x00FFFFFFu) + countOneBits(child_mask & ((1u << octant) - 1u));
        if (level == 0u) {
            break;
        }
        level -= 1u;
    }
    let node = nodes[node_index];
    if (node == SOLID_LEAF_MARKER) {
        return vec2<u32>(1u, 0u);
    }
    return vec2<u32>(2u + (node & 0x00FFFFFFu), 0u);
}

// Ray-AABB intersection (t_near, t_far); a miss has t_near > t_far.
fn intersect_aabb(ro: vec3<f32>, inv_dir: vec3<f32>, bmin: vec3<f32>, bmax: vec3<f32>) -> vec2<f32> {
    let t1 = (bmin - ro) * inv_dir;
    let t2 = (bmax - ro) * inv_dir;
    let tmin = min(t1, t2);
    let tmax = max(t1, t2);
    return vec2<f32>(max(max(tmin.x, tmin.y), tmin.z), min(min(tmax.x, tmax.y), tmax.z));
}

// Gray in free space, orange when the camera is inside solid voxels (the march
// is inverted to find the way out). Face-axis brightness and darker, constant
// pixel-width edges keep the grid readable at any distance.
fn shade(hit: vec3<f32>, vox_min: vec3<f32>) -> vec4<f32> {
    let res = uniforms.voxel_res;
    var pixel = uniforms.pixel_scale;
    if (uniforms.orthographic == 0u) {
        pixel *= length(hit - uniforms.eye);
    }
    let edge_width = clamp(EDGE_PIXELS * pixel / res, 0.01, 0.5);
    // Distant voxels span few pixels; their edges alias into noise, so fade
    // them out and keep the face fill.
    let edge_fade = smoothstep(3.0, 8.0, res / pixel);

    let local = (hit - vox_min) / res;
    let f = min(local, vec3<f32>(1.0) - local);
    // On a face one component is ~0; the median is the distance to the nearest edge.
    let edge_dist = max(min(f.x, f.y), min(max(f.x, f.y), f.z));
    let edge = (1.0 - smoothstep(0.0, edge_width, edge_dist)) * edge_fade;

    var brightness = 0.85;
    if (f.y <= f.x && f.y <= f.z) {
        brightness = 0.65;
    } else if (f.z <= f.x) {
        brightness = 0.45;
    }
    let tint = select(vec3<f32>(1.0), vec3<f32>(1.0, 0.35, 0.08), uniforms.inverted != 0u);
    // SuperSplat's values are display (sRGB) colors; targets here are linear.
    let color = pow(tint * mix(brightness, 0.2, edge), vec3<f32>(2.2));
    let alpha = mix(FILL_ALPHA, EDGE_ALPHA, edge);
    return vec4<f32>(color * alpha, alpha);
}

fn leaf_solid(mask_lo: u32, mask_hi: u32, v: vec3<i32>) -> bool {
    let bit = u32(v.z) * 16u + u32(v.y) * 4u + u32(v.x);
    if (bit < 32u) {
        return (mask_lo & (1u << bit)) != 0u;
    }
    return (mask_hi & (1u << (bit - 32u))) != 0u;
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = (position.xy - uniforms.viewport.xy) / uniforms.viewport.zw;
    let ndc = vec2<f32>(pixel.x * 2.0 - 1.0, 1.0 - pixel.y * 2.0);
    // Reverse-Z: depth 1 is the near plane. Perspective rays leave the eye
    // through it; orthographic rays start on it and run along -Z.
    let near = uniforms.view_from_clip * vec4<f32>(ndc, 1.0, 1.0);
    let near_view = near.xyz / near.w;
    var ro = (uniforms.grid_from_view * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    var rd = normalize((uniforms.grid_from_view * vec4<f32>(near_view, 0.0)).xyz);
    if (uniforms.orthographic != 0u) {
        ro = (uniforms.grid_from_view * vec4<f32>(near_view, 1.0)).xyz;
        rd = normalize((uniforms.grid_from_view * vec4<f32>(0.0, 0.0, -1.0, 0.0)).xyz);
    }

    let grid_min = uniforms.grid_min;
    let res = uniforms.voxel_res;
    let grid_max = grid_min + vec3<f32>(uniforms.voxels) * res;
    let inv_dir = 1.0 / rd;
    let grid_hit = intersect_aabb(ro, inv_dir, grid_min, grid_max);
    if (max(grid_hit.x, 0.0) > grid_hit.y) {
        return vec4<f32>(0.0);
    }

    let block_res = res * f32(LEAF_SIZE);
    let num_blocks = vec3<i32>(uniforms.voxels / vec3<u32>(u32(LEAF_SIZE)));
    let entry = ro + rd * (max(grid_hit.x, 0.0) + 0.0001);
    var b = clamp(vec3<i32>(floor((entry - grid_min) / block_res)), vec3<i32>(0), num_blocks - 1);

    let step = select(vec3<i32>(-1), vec3<i32>(1), rd >= vec3<f32>(0.0));
    let t_delta = abs(block_res * inv_dir);
    let block_min = grid_min + vec3<f32>(b) * block_res;
    let next_bound = select(block_min, block_min + block_res, rd >= vec3<f32>(0.0));
    var t_max = (next_bound - ro) * inv_dir;

    let inv = uniforms.inverted != 0u;
    let v_delta = abs(res * inv_dir);

    for (var i = 0u; i < MAX_STEPS; i++) {
        let q = query_block(b);
        let result = q.x;
        let skip_level = q.y;
        // Skip uniform cells: empty normally, solid when inverted.
        let skip = select(result == 0u, result == 1u, inv) && skip_level >= 1u;

        if (skip) {
            let cell = i32(1u << skip_level);
            let cell_min = b & vec3<i32>(~(cell - 1));
            for (var s = 0u; s < 128u; s++) {
                if (t_max.x < t_max.y && t_max.x < t_max.z) {
                    b.x += step.x;
                    t_max.x += t_delta.x;
                    if (b.x < cell_min.x || b.x >= cell_min.x + cell) { break; }
                } else if (t_max.y < t_max.z) {
                    b.y += step.y;
                    t_max.y += t_delta.y;
                    if (b.y < cell_min.y || b.y >= cell_min.y + cell) { break; }
                } else {
                    b.z += step.z;
                    t_max.z += t_delta.z;
                    if (b.z < cell_min.z || b.z >= cell_min.z + cell) { break; }
                }
            }
        } else {
            if (select(result != 0u, result != 1u, inv)) {
                let origin = grid_min + vec3<f32>(b) * block_res;
                let b_hit = intersect_aabb(ro, inv_dir, origin, origin + vec3<f32>(block_res));
                let local = (ro + rd * (max(b_hit.x, 0.0) + 0.0001) - origin) / res;
                var v = clamp(vec3<i32>(floor(local)), vec3<i32>(0), vec3<i32>(LEAF_SIZE - 1));
                let vox_origin = origin + vec3<f32>(v) * res;
                let v_next = select(vox_origin, vox_origin + res, rd >= vec3<f32>(0.0));
                var v_max = (v_next - ro) * inv_dir;

                var mask_lo = 0u;
                var mask_hi = 0u;
                if (result > 1u) {
                    mask_lo = leaf_data[(result - 2u) * 2u];
                    mask_hi = leaf_data[(result - 2u) * 2u + 1u];
                }

                for (var j = 0u; j < 12u; j++) {
                    let solid = result == 1u || leaf_solid(mask_lo, mask_hi, v);
                    if (solid != inv) {
                        let vox_min = origin + vec3<f32>(v) * res;
                        let hit = intersect_aabb(ro, inv_dir, vox_min, vox_min + vec3<f32>(res));
                        return shade(ro + rd * max(hit.x, 0.0), vox_min);
                    }
                    if (v_max.x < v_max.y && v_max.x < v_max.z) {
                        v.x += step.x;
                        v_max.x += v_delta.x;
                        if (v.x < 0 || v.x >= LEAF_SIZE) { break; }
                    } else if (v_max.y < v_max.z) {
                        v.y += step.y;
                        v_max.y += v_delta.y;
                        if (v.y < 0 || v.y >= LEAF_SIZE) { break; }
                    } else {
                        v.z += step.z;
                        v_max.z += v_delta.z;
                        if (v.z < 0 || v.z >= LEAF_SIZE) { break; }
                    }
                }
            }
            if (t_max.x < t_max.y && t_max.x < t_max.z) {
                b.x += step.x;
                t_max.x += t_delta.x;
            } else if (t_max.y < t_max.z) {
                b.y += step.y;
                t_max.y += t_delta.y;
            } else {
                b.z += step.z;
                t_max.z += t_delta.z;
            }
        }
        if (any(b < vec3<i32>(0)) || any(b >= num_blocks)) {
            break;
        }
    }
    return vec4<f32>(0.0);
}
