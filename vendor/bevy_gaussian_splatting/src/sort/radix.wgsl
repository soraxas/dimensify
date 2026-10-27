#import bevy_gaussian_splatting::bindings::{
    view,
    globals,
    gaussian_uniforms,
    sorting_pass_index,
    sorting,
    status_counters,
    draw_indirect,
    input_entries,
    output_entries,
    sorted_entries,
    DrawIndirect,
    Entry,
}
#import bevy_gaussian_splatting::transform::{
    world_to_clip,
    in_frustum,
    splat_center_is_projectable,
}

fn sort_candidate(world: vec3<f32>) -> bool {
#ifdef GAUSSIAN_3D
    // No covariance is available in this position-only stage. Retain all
    // projectable centres rather than lose visible broad Gaussian footprints.
    // Shared projected-support compaction is a separate optimization.
    return splat_center_is_projectable(world);
#else
    return in_frustum(world_to_clip(world).xyz);
#endif
}

#ifdef SOG_PROJECTED
#import bevy_gaussian_splatting::bindings::projected_records
#else
#ifdef PACKED_F32
#import bevy_gaussian_splatting::packed::get_position
#else

#ifdef BUFFER_STORAGE
#import bevy_gaussian_splatting::planar::get_position
#endif

#endif

#ifdef BUFFER_TEXTURE
#import bevy_gaussian_splatting::texture::get_position
#endif
#endif

fn candidate_key(index: u32) -> u32 {
#ifdef SOG_PROJECTED
    return projected_records[index].centre_key.w;
#else
    let world = (gaussian_uniforms.transform * vec4<f32>(get_position(index), 1.0)).xyz;
    if !sort_candidate(world) { return 0xFFFFFFFFu; }
    let delta = world - view.world_position;
    return 0xFFFFFFFFu - bitcast<u32>(dot(delta, delta));
#endif
}

struct SortingGlobal {
    digit_histogram: array<array<atomic<u32>, #{RADIX_BASE}>, #{RADIX_DIGIT_PLACES}>,
    assignment_counter: atomic<u32>,
    digit_tile_head: array<atomic<u32>, #{RADIX_BASE}>,
    dispatch_x: u32,
    dispatch_y: u32,
    dispatch_z: u32,
}

@group(3) @binding(0) var<uniform> sorting_pass_index: u32;
@group(3) @binding(1) var<storage, read_write> sorting: SortingGlobal;
// Per-tile temporary storage for radix pass C.
@group(3) @binding(2) var<storage, read_write> status_counters: array<array<atomic<u32>, #{RADIX_BASE}>>;
@group(3) @binding(3) var<storage, read_write> draw_indirect: DrawIndirect;
@group(3) @binding(4) var<storage, read_write> input_entries: array<Entry>;
@group(3) @binding(5) var<storage, read_write> output_entries: array<Entry>;

// The selected attribute count remains in the uniform; compacted sort bounds
// use the GPU survivor count throughout every digit pass.
fn sort_count() -> u32 {
    if (gaussian_uniforms.visibility_compaction != 0u) {
        return atomicLoad(&draw_indirect.instance_count);
    }
    return gaussian_uniforms.count;
}

@compute @workgroup_size(#{WORKGROUP_INVOCATIONS_A})
fn radix_reset(
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(global_invocation_id) global_id: vec3<u32>,
){
    let b = local_id.x;
    for (var p = 0u; p < #{RADIX_DIGIT_PLACES}u; p += 1u) {
        atomicStore(&sorting.digit_histogram[p][b], 0u);
    }
    atomicStore(&sorting.digit_tile_head[b], 0u);
    if (global_id.x == 0u) {
        atomicStore(&sorting.assignment_counter, 0u);
        draw_indirect.vertex_count = 4u;
        atomicStore(&draw_indirect.instance_count, 0u);
    }
}

@compute @workgroup_size(#{WORKGROUP_INVOCATIONS_A})
fn radix_sort_a(
    @builtin(local_invocation_id) gl_LocalInvocationID: vec3<u32>,
    @builtin(global_invocation_id) gl_GlobalInvocationID: vec3<u32>,
) {
    if (gl_LocalInvocationID.x == 0u && gl_LocalInvocationID.y == 0u && gl_GlobalInvocationID.x == 0u) {
        draw_indirect.vertex_count = 4u;
        atomicStore(&draw_indirect.instance_count, gaussian_uniforms.count);
    }
    workgroupBarrier();

    let thread_index = gl_GlobalInvocationID.x;
    let start_entry_index = thread_index * #{ENTRIES_PER_INVOCATION_A}u;
    let end_entry_index = start_entry_index + #{ENTRIES_PER_INVOCATION_A}u;

    for (var entry_index = start_entry_index; entry_index < end_entry_index; entry_index += 1u) {
        if (entry_index >= gaussian_uniforms.count) { continue; }
        let key = candidate_key(entry_index) >> #{RADIX_KEY_SHIFT}u;
        input_entries[entry_index].key = key;
        input_entries[entry_index].value = entry_index;
        for(var shift = 0u; shift < #{RADIX_DIGIT_PLACES}u; shift += 1u) {
            let digit = (key >> (shift * #{RADIX_BITS_PER_DIGIT}u)) & (#{RADIX_BASE}u - 1u);
            atomicAdd(&sorting.digit_histogram[shift][digit], 1u);
        }
    }
}

@compute @workgroup_size(1)
fn radix_sort_b(
    @builtin(global_invocation_id) gl_GlobalInvocationID: vec3<u32>,
) {
    if (gl_GlobalInvocationID.y == 0u) {
        sorting.dispatch_x = 1u;
        sorting.dispatch_y = (sort_count() + #{WORKGROUP_ENTRIES_C}u - 1u) / #{WORKGROUP_ENTRIES_C}u;
        sorting.dispatch_z = 1u;
    }
    var sum = 0u;
    for(var digit = 0u; digit < #{RADIX_BASE}u; digit += 1u) {
        let tmp = atomicLoad(&sorting.digit_histogram[gl_GlobalInvocationID.y][digit]);
        atomicStore(&sorting.digit_histogram[gl_GlobalInvocationID.y][digit], sum);
        sum += tmp;
    }
}

// Each lane owns a consecutive run, so prefixing lane counts and walking each
// run in order preserves source order even for identical depth keys.
var<workgroup> visible_lane_counts: array<u32, #{WORKGROUP_INVOCATIONS_C}>;

@compute @workgroup_size(#{WORKGROUP_INVOCATIONS_C})
fn compact_count(@builtin(local_invocation_id) local: vec3<u32>,
                 @builtin(workgroup_id) group: vec3<u32>) {
    let start = group.y * #{WORKGROUP_ENTRIES_C}u + local.x * #{ENTRIES_PER_INVOCATION_C}u;
    var visible = 0u;
    for (var j = 0u; j < #{ENTRIES_PER_INVOCATION_C}u; j += 1u) {
        let idx = start + j;
        if (idx >= gaussian_uniforms.count) { continue; }
        let key = candidate_key(idx);
        var entry = Entry(0u, 0xFFFFFFFFu);
        if (key != 0xFFFFFFFFu) {
            entry = Entry(key >> #{RADIX_KEY_SHIFT}u, idx);
            visible += 1u;
        }
        // Opposite ping-pong storage is safe until the following scatter completes.
        output_entries[idx] = entry;
    }
    visible_lane_counts[local.x] = visible;
    workgroupBarrier();
    if (local.x == 0u) {
        var total = 0u;
        for (var lane = 0u; lane < #{WORKGROUP_INVOCATIONS_C}u; lane += 1u) {
            total += visible_lane_counts[lane];
        }
        atomicStore(&status_counters[group.y][0], total);
    }
}

@compute @workgroup_size(1)
fn compact_scan() {
    let tiles = (gaussian_uniforms.count + #{WORKGROUP_ENTRIES_C}u - 1u) / #{WORKGROUP_ENTRIES_C}u;
    var sum = 0u;
    for (var tile = 0u; tile < tiles; tile += 1u) {
        let count = atomicLoad(&status_counters[tile][0]);
        atomicStore(&status_counters[tile][0], sum);
        sum += count;
    }
    atomicStore(&draw_indirect.instance_count, sum);
}

@compute @workgroup_size(#{WORKGROUP_INVOCATIONS_C})
fn compact_scatter(@builtin(local_invocation_id) local: vec3<u32>,
                   @builtin(workgroup_id) group: vec3<u32>) {
    let start = group.y * #{WORKGROUP_ENTRIES_C}u + local.x * #{ENTRIES_PER_INVOCATION_C}u;
    var visible = 0u;
    for (var j = 0u; j < #{ENTRIES_PER_INVOCATION_C}u; j += 1u) {
        let idx = start + j;
        if (idx < gaussian_uniforms.count) {
            if (output_entries[idx].value != 0xFFFFFFFFu) { visible += 1u; }
        }
    }
    visible_lane_counts[local.x] = visible;
    workgroupBarrier();
    if (local.x == 0u) {
        var sum = atomicLoad(&status_counters[group.y][0]);
        for (var lane = 0u; lane < #{WORKGROUP_INVOCATIONS_C}u; lane += 1u) {
            let count = visible_lane_counts[lane];
            visible_lane_counts[lane] = sum;
            sum += count;
        }
    }
    workgroupBarrier();
    var dst = visible_lane_counts[local.x];
    for (var j = 0u; j < #{ENTRIES_PER_INVOCATION_C}u; j += 1u) {
        let idx = start + j;
        if (idx >= gaussian_uniforms.count) { continue; }
        let entry = output_entries[idx];
        if (entry.value == 0xFFFFFFFFu) { continue; }
        input_entries[dst] = entry;
        dst += 1u;
        for (var shift = 0u; shift < #{RADIX_DIGIT_PLACES}u; shift += 1u) {
            let digit = (entry.key >> (shift * #{RADIX_BITS_PER_DIGIT}u)) & (#{RADIX_BASE}u - 1u);
            atomicAdd(&sorting.digit_histogram[shift][digit], 1u);
        }
    }
}

// --- SHARED MEMORY for radix pass C ---
var<workgroup> tile_digit_counts: array<atomic<u32>, #{RADIX_BASE}>;
const MASK_WORDS: u32 = #{WORKGROUP_INVOCATIONS_C}u / 32u;
const DIGIT_MASK_WORDS: u32 = #{RADIX_BASE}u * MASK_WORDS;
var<workgroup> digit_masks: array<atomic<u32>, DIGIT_MASK_WORDS>;
var<workgroup> digit_round_offsets: array<u32, #{RADIX_BASE}>;

@compute @workgroup_size(#{WORKGROUP_INVOCATIONS_C})
fn radix_sort_c_count_tiles(
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let tid = local_id.x;
    let tile_size = #{WORKGROUP_ENTRIES_C}u;
    let threads = #{WORKGROUP_INVOCATIONS_C}u;
    let global_entry_offset = workgroup_id.y * tile_size;

    if (tid < #{RADIX_BASE}u) {
        atomicStore(&tile_digit_counts[tid], 0u);
    }
    workgroupBarrier();

    for (var i = tid; i < tile_size; i += threads) {
        let idx = global_entry_offset + i;
        if (idx >= sort_count()) {
            continue;
        }

        let entry = input_entries[idx];
        let digit = (entry.key >> (sorting_pass_index * #{RADIX_BITS_PER_DIGIT}u)) & (#{RADIX_BASE}u - 1u);
        atomicAdd(&tile_digit_counts[digit], 1u);
    }
    workgroupBarrier();

    if (tid < #{RADIX_BASE}u) {
        let count = atomicLoad(&tile_digit_counts[tid]);
        atomicStore(&status_counters[workgroup_id.y][tid], count);
    }
}

// One workgroup of RADIX_BASE lanes, lane = digit. Each digit's tile-prefix is
// independent (no cross-lane data), so this packs 256 lanes into ~4 full waves instead
// of dispatching 256 single-lane workgroups (1/64 wave occupancy on RDNA).
@compute @workgroup_size(#{RADIX_BASE})
fn radix_sort_c_scan_tiles(
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    let digit = local_id.x;
    if (digit >= #{RADIX_BASE}u) {
        return;
    }

    let tile_size = #{WORKGROUP_ENTRIES_C}u;
    let tile_count = (sort_count() + tile_size - 1u) / tile_size;

    var sum = atomicLoad(&sorting.digit_histogram[sorting_pass_index][digit]);
    for (var tile = 0u; tile < tile_count; tile += 1u) {
        let count = atomicLoad(&status_counters[tile][digit]);
        atomicStore(&status_counters[tile][digit], sum);
        sum += count;
    }
}

@compute @workgroup_size(#{WORKGROUP_INVOCATIONS_C})
fn radix_sort_c_scatter(
    @builtin(local_invocation_id) local_id: vec3<u32>,
    @builtin(workgroup_id) workgroup_id: vec3<u32>,
) {
    let tid = local_id.x;
    let tile_size = #{WORKGROUP_ENTRIES_C}u;
    let threads = #{WORKGROUP_INVOCATIONS_C}u;
    let global_entry_offset = workgroup_id.y * tile_size;

    // Portable stable ranked scatter, following PlayCanvas's mask/popcount
    // approach (MIT; see radix-scatter-LICENSE.txt), generalized to our existing
    // 8-bit digits. No subgroup assumptions, serial tile placement or extra
    // full-tile arrays. Keep full key precision and global tile-prefix ordering.
    for (var i = tid; i < DIGIT_MASK_WORDS; i += threads) {
        atomicStore(&digit_masks[i], 0u);
    }
    if (tid < #{RADIX_BASE}u) {
        digit_round_offsets[tid] = 0u;
    }
    workgroupBarrier();

    let word = tid / 32u;
    let bit = tid % 32u;
    let count = sort_count();
    for (var round = 0u; round < #{ENTRIES_PER_INVOCATION_C}u; round += 1u) {
        let idx = global_entry_offset + round * threads + tid;
        let valid = idx < count;
        var entry = Entry(0u, 0u);
        var digit = 0u;
        if (valid) {
            entry = input_entries[idx];
            digit = (entry.key >> (sorting_pass_index * #{RADIX_BITS_PER_DIGIT}u)) & (#{RADIX_BASE}u - 1u);
            atomicOr(&digit_masks[digit * MASK_WORDS + word], 1u << bit);
        }
        workgroupBarrier();

        if (valid) {
            let base = digit * MASK_WORDS;
            var rank = digit_round_offsets[digit];
            for (var w = 0u; w < word; w += 1u) {
                rank += countOneBits(atomicLoad(&digit_masks[base + w]));
            }
            rank += countOneBits(atomicLoad(&digit_masks[base + word]) & ((1u << bit) - 1u));
            let dst = atomicLoad(&status_counters[workgroup_id.y][digit]) + rank;
            output_entries[dst] = entry;
        }

        if (round + 1u < #{ENTRIES_PER_INVOCATION_C}u) {
            // All readers must finish before masks are cleared. Tail lanes
            // participate in every barrier; even all-ones keys remain valid.
            workgroupBarrier();
            if (tid < #{RADIX_BASE}u) {
                var total = 0u;
                for (var w = 0u; w < MASK_WORDS; w += 1u) {
                    let at = tid * MASK_WORDS + w;
                    total += countOneBits(atomicLoad(&digit_masks[at]));
                    atomicStore(&digit_masks[at], 0u);
                }
                digit_round_offsets[tid] += total;
            }
            workgroupBarrier();
        }
    }
}
