// Composable per-splat animation stages ("load effects").
//
// Every stage is a pure function of the splat's rest position (world space),
// a per-splat hash and the effect clock. Stages read the immutable rest
// position and accumulate into one `SplatEffect`, so they compose in any
// combination without one stage's displacement feeding another's field.
// Stages are selected at run time by `gaussian_uniforms.effect_mask`; the
// raster pipeline compiles this module only under `SPLAT_EFFECTS`.
//
// Adding a stage: write `fn <name>_stage(rest, hash, effect) -> SplatEffect`,
// give it the next mask bit here and in `SplatEffects::mask` (Rust), and call
// it from `apply_effects`.
import bevy_gaussian_splatting::render::bindings::gaussian_uniforms;

const EFFECT_REVEAL: u32 = 1u;
const EFFECT_EXPLOSION: u32 = 2u;

struct SplatEffect {
    // World-space centre to project.
    position: vec3<f32>,
    // Linear scale applied to the projected footprint.
    scale: f32,
    // Multiplier on the splat opacity.
    opacity: f32,
    // Linear RGB added to the shaded colour.
    emission: vec3<f32>,
}

fn identity_effect(rest: vec3<f32>) -> SplatEffect {
    return SplatEffect(rest, 1.0, 1.0, vec3<f32>(0.0));
}

// Position-keyed so the stagger survives generation swaps that remap indices.
fn effect_hash(position: vec3<f32>) -> vec3<f32> {
    let bits = bitcast<vec3<u32>>(position);
    var x = bits.x ^ (bits.y * 0x9e3779b9u) ^ (bits.z * 0x85ebca6bu);
    x = (x ^ (x >> 16u)) * 0x7feb352du;
    x = (x ^ (x >> 15u)) * 0x846ca68bu;
    x = x ^ (x >> 16u);
    let y = (x ^ 0x68e31da4u) * 0x7feb352du;
    let z = (y ^ (y >> 15u)) * 0x846ca68bu;
    return vec3<f32>(
        f32(x >> 8u),
        f32(y >> 8u),
        f32(z >> 8u),
    ) / 16777216.0;
}

// Scale a projected covariance about its low-pass floor, so a shrunk splat
// becomes a pixel-sized sparkle rather than vanishing below raster coverage.
fn scale_cov2d(cov: vec3<f32>, scale: f32) -> vec3<f32> {
    // Keep unanimated footprints bit-exact.
    if scale == 1.0 {
        return cov;
    }
    let low_pass = vec3<f32>(0.3, 0.0, 0.3);
    return (cov - low_pass) * (scale * scale) + low_pass;
}

// A glowing front sweeps outward from the origin: across the ground plane, or
// as a sphere when the axes weight all three components (camera origin).
// Ahead of it splats are dim, shimmering sparkles; behind it they settle.
fn reveal_stage(rest: vec3<f32>, hash: vec3<f32>, effect: SplatEffect) -> SplatEffect {
    var result = effect;
    let origin = gaussian_uniforms.effect_origin.xyz;
    let extent = max(gaussian_uniforms.effect_origin.w, 1e-4);
    let duration = max(gaussian_uniforms.reveal_duration, 1e-4);
    let width = max(gaussian_uniforms.reveal_edge, 1e-3);
    let offset = rest - origin;
    // Start the front at the nearest scene point, so a camera origin does not
    // spend the animation crossing empty space between the eye and the scene.
    let near = gaussian_uniforms.effect_axes.w;
    let radial = max(length(offset * gaussian_uniforms.effect_axes.xyz) - near, 0.0)
        / max(extent - near, 1e-4);
    // `extent` is the farthest weighted distance, so the front clears it fully.
    let front = gaussian_uniforms.effect_time / duration * (1.0 + 2.0 * width) - width;
    let ahead = radial - front;
    let revealed = 1.0 - smoothstep(-width, 0.0, ahead);
    let edge = exp(-abs(ahead) / (0.35 * width));
    let t = gaussian_uniforms.effect_time;
    let shimmer = 0.5 + 0.5 * sin(t * 4.0 + hash.x * 6.2831853);
    let jitter = (hash - 0.5) * (0.03 * extent) * (1.0 - revealed)
        * sin(t * 2.0 + hash.yzx * 6.2831853);
    // Pinch toward the origin at the front, as if the surface is drawn in.
    result.position = result.position + jitter - offset * (0.08 * edge);
    result.scale = result.scale * mix(0.04, 1.0, revealed);
    result.opacity = result.opacity * mix(0.25 * shimmer, 1.0, revealed);
    result.emission = result.emission
        + gaussian_uniforms.effect_glow.rgb * (gaussian_uniforms.effect_glow.a * edge);
    return result;
}

// Splats burst out of the origin along a staggered arc and grow into place.
fn explosion_stage(rest: vec3<f32>, hash: vec3<f32>, effect: SplatEffect) -> SplatEffect {
    var result = effect;
    let origin = gaussian_uniforms.effect_origin.xyz;
    let extent = max(gaussian_uniforms.effect_origin.w, 1e-4);
    let duration = max(gaussian_uniforms.explosion_duration, 1e-4);
    let stagger = 0.35;
    let start = hash.x * stagger;
    let progress = clamp(
        (gaussian_uniforms.effect_time / duration - start) / (1.0 - stagger),
        0.0,
        1.0,
    );
    // Ease-out with a slight overshoot so splats land with a settle.
    let p = 1.0 - progress;
    let ease = 1.0 - p * p * p + 0.12 * sin(3.1415927 * progress) * p;
    let arc = vec3<f32>(0.0, 1.0, 0.0) * (0.25 * extent * (0.5 + hash.y))
        * sin(3.1415927 * progress);
    result.position = result.position + (origin - rest) * (1.0 - ease) + arc;
    result.scale = result.scale * smoothstep(0.0, 0.6, progress);
    result.opacity = result.opacity * smoothstep(0.0, 0.2, progress);
    result.emission = result.emission
        + gaussian_uniforms.effect_glow.rgb
        * (gaussian_uniforms.effect_glow.a * 0.6 * (1.0 - progress) * step(1e-4, progress));
    return result;
}

fn apply_effects(rest: vec3<f32>) -> SplatEffect {
    var effect = identity_effect(rest);
    let mask = gaussian_uniforms.effect_mask;
    if mask == 0u {
        return effect;
    }
    let hash = effect_hash(rest);
    if (mask & EFFECT_EXPLOSION) != 0u {
        effect = explosion_stage(rest, hash, effect);
    }
    if (mask & EFFECT_REVEAL) != 0u {
        effect = reveal_stage(rest, hash, effect);
    }
    return effect;
}
