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
import bevy_gaussian_splatting::render::helpers::cov2d;

const EFFECT_REVEAL: u32 = 1u;
const EFFECT_EXPLOSION: u32 = 2u;
const EFFECT_MAGIC: u32 = 4u;
const EFFECT_SPREAD: u32 = 8u;
const EFFECT_RIPPLE: u32 = 16u;
// Modifies EFFECT_MAGIC: open with the azimuth wipe before the front.
const EFFECT_MAGIC_WIPE: u32 = 32u;
const EFFECT_STARS: u32 = 64u;

// Neutral grey that `SplatEffect::color_mix` blends from.
const EFFECT_GREY: vec3<f32> = vec3<f32>(0.3);

struct SplatEffect {
    // World-space centre to project.
    position: vec3<f32>,
    // Linear scale applied to the projected footprint.
    scale: f32,
    // Multiplier on the splat opacity.
    opacity: f32,
    // Linear RGB added to the shaded colour.
    emission: vec3<f32>,
    // Blend from `EFFECT_GREY` (0) to the shaded colour (1).
    color_mix: f32,
    // rgb colour mixed over the shaded colour with weight a, before emission.
    tint: vec4<f32>,
    // Blend from the splat's own footprint (0) to an isotropic point (1) of
    // world-space standard deviation `point_size`.
    point: f32,
    point_size: f32,
}

fn identity_effect(rest: vec3<f32>) -> SplatEffect {
    return SplatEffect(rest, 1.0, 1.0, vec3<f32>(0.0), 1.0, vec4<f32>(0.0), 0.0, 0.0);
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

// Smooth value noise in [0, 1)^3 over a unit lattice, for coherent drift.
fn effect_noise(p: vec3<f32>) -> vec3<f32> {
    // `+ 0.0` folds -0 into +0 so the lattice hash has no seam at zero.
    let i = floor(p) + 0.0;
    let f = fract(p);
    let w = f * f * (3.0 - 2.0 * f);
    let x00 = mix(effect_hash(i), effect_hash(i + vec3<f32>(1.0, 0.0, 0.0)), w.x);
    let x10 = mix(effect_hash(i + vec3<f32>(0.0, 1.0, 0.0)), effect_hash(i + vec3<f32>(1.0, 1.0, 0.0)), w.x);
    let x01 = mix(effect_hash(i + vec3<f32>(0.0, 0.0, 1.0)), effect_hash(i + vec3<f32>(1.0, 0.0, 1.0)), w.x);
    let x11 = mix(effect_hash(i + vec3<f32>(0.0, 1.0, 1.0)), effect_hash(i + vec3<f32>(1.0, 1.0, 1.0)), w.x);
    return mix(mix(x00, x10, w.y), mix(x01, x11, w.y), w.z);
}

// Weighted distance from the effect origin: 0 at the nearest scene point,
// 1 at `extent`. The extent may be capped short of far floaters; clamping
// lets those settle with the last of the scene instead of staying hidden.
fn effect_radial(offset: vec3<f32>) -> f32 {
    let extent = max(gaussian_uniforms.effect_origin.w, 1e-4);
    let near = gaussian_uniforms.effect_axes.w;
    return min(
        max(length(offset * gaussian_uniforms.effect_axes.xyz) - near, 0.0)
            / max(extent - near, 1e-4),
        1.0,
    );
}

// Shaded colour under an effect: blended from grey, then lit by emission.
fn effect_color(rgb: vec3<f32>, effect: SplatEffect) -> vec3<f32> {
    var color = rgb;
    // Keep unanimated colours bit-exact.
    if effect.color_mix != 1.0 {
        color = mix(EFFECT_GREY, color, effect.color_mix);
    }
    if effect.tint.a != 0.0 {
        color = mix(color, effect.tint.rgb, effect.tint.a);
    }
    return color + effect.emission;
}

// Projected footprint under an effect: the splat's own covariance scaled, or
// blended toward a fixed world-size point at the effect's centre.
fn effect_cov2d(cov: vec3<f32>, position: vec3<f32>, effect: SplatEffect) -> vec3<f32> {
    let scaled = scale_cov2d(cov, effect.scale);
    // Keep unanimated footprints bit-exact.
    if effect.point == 0.0 {
        return scaled;
    }
    let s2 = effect.point_size * effect.point_size;
    let point = cov2d(position, array<f32, 6>(s2, 0.0, 0.0, s2, 0.0, s2));
    return mix(scaled, point, effect.point);
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
    let radial = effect_radial(offset);
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

// Spark's "Magic" reveal. With the wipe bit, a wipe first circles the vertical
// axis through the origin, conjuring pixel-sized sparkles that drift on
// coherent noise, with a glowing hand at the wipe. A glowing front then grows
// outward, drawing the surface in and settling sparkles into full splats
// behind it. Only a hashed fifth of the unsettled splats sparkle, so dense
// scenes and their floaters do not fill the view with noise.
fn magic_stage(rest: vec3<f32>, hash: vec3<f32>, effect: SplatEffect) -> SplatEffect {
    var result = effect;
    let origin = gaussian_uniforms.effect_origin.xyz;
    let extent = max(gaussian_uniforms.effect_origin.w, 1e-4);
    let duration = max(gaussian_uniforms.magic_duration, 1e-4);
    let t = gaussian_uniforms.effect_time;
    let u = t / duration;
    let offset = rest - origin;
    var swept = 1.0;
    var hand_glow = 0.0;
    var p = clamp(u, 0.0, 1.0);
    if (gaussian_uniforms.effect_mask & EFFECT_MAGIC_WIPE) != 0u {
        // Wipe over the first 35%: azimuth and hand both span [-1, 1].
        let azimuth = atan2(offset.x, offset.z) / 3.1415927;
        let hand = u / 0.35 * 2.0 - 1.0;
        swept = step(azimuth, hand);
        hand_glow = 0.5 * exp(-50.0 * abs(hand - azimuth));
        // The front then starts at 40%.
        p = clamp((u - 0.4) / 0.6, 0.0, 1.0);
    }
    // The front accelerates like Spark's eased start. Sparkles settle across
    // `band` behind the glowing edge, which clears `extent` at the end.
    let band = 0.25;
    let edge = p * (0.4 + 0.6 * p) * (1.0 + 2.0 * band) - band;
    let radial = effect_radial(offset);
    let ahead = smoothstep(edge - band, edge, radial);
    let ring = exp(-30.0 * abs(radial - edge));
    let drift = (effect_noise(offset / extent * 3.0 + t * 0.5) - 0.5) * (0.07 * extent);
    result.position = result.position - offset * (0.2 * ring) + drift * ahead;
    let sparkles = step(hash.y, 0.2);
    result.scale = result.scale * (1.0 - ahead);
    result.opacity = result.opacity * max(swept * mix(1.0, sparkles, ahead), hand_glow);
    result.emission = result.emission
        + gaussian_uniforms.effect_glow.rgb * (gaussian_uniforms.effect_glow.a * (ring + hand_glow));
    return result;
}

// After Spark's "Spread" reveal. The scene starts squeezed toward the origin
// along the weighted axes and relaxes outward on an accelerating clock, while
// one wide radial wave grows each splat from nothing to full size, fading it
// from grey to its colour on the way. Spark uses three separate waves; one
// continuous wave avoids their visible steps and see-through plateau.
fn spread_stage(rest: vec3<f32>, hash: vec3<f32>, effect: SplatEffect) -> SplatEffect {
    var result = effect;
    let origin = gaussian_uniforms.effect_origin.xyz;
    let duration = max(gaussian_uniforms.spread_duration, 1e-4);
    let u = clamp(gaussian_uniforms.effect_time / duration, 0.0, 1.0);
    let offset = rest - origin;
    let squeeze = min(1.0, 0.3 + 0.7 * u * u / 0.85);
    // The wave keys on the rest distance, so it rides outward with the
    // relaxing scene. Each splat grows across a band of 45% of the duration;
    // the farthest finish at 95%.
    let radial = effect_radial(offset);
    let g = clamp((u - 0.05 - 0.45 * radial) / 0.45, 0.0, 1.0);
    let grow = g * g * (3.0 - 2.0 * g);
    let colour = smoothstep(0.3, 0.8, g);
    result.position = result.position
        - offset * gaussian_uniforms.effect_axes.xyz * (1.0 - squeeze);
    result.scale = result.scale * grow;
    result.opacity = result.opacity * smoothstep(0.0, 0.2, g) * mix(0.35, 1.0, colour);
    result.color_mix = result.color_mix * colour;
    return result;
}

// Click ripples: a glowing ring runs from each origin out to its radius,
// nudging splats outward and swelling them as it passes, with a fainter echo
// behind. Splats at or past the radius are untouched.
fn ripple_stage(rest: vec3<f32>, hash: vec3<f32>, effect: SplatEffect) -> SplatEffect {
    var result = effect;
    let duration = max(gaussian_uniforms.ripple_duration, 1e-4);
    for (var i = 0u; i < 4u; i = i + 1u) {
        let age = gaussian_uniforms.ripple_ages[i];
        if age < 0.0 || age >= duration {
            continue;
        }
        let ripple = gaussian_uniforms.ripples[i];
        let radius = max(ripple.w, 1e-4);
        let offset = rest - ripple.xyz;
        let distance = length(offset);
        if distance >= radius {
            continue;
        }
        let progress = age / duration;
        // Ease-out: the ring leaves fast and slows as it reaches the radius.
        let front = (1.0 - (1.0 - progress) * (1.0 - progress)) * radius;
        let width = 0.08 * radius;
        let wave = exp(-abs(distance - front) / width)
            + 0.4 * exp(-abs(distance - 0.6 * front) / width);
        // Fades out over time and to zero at the radius.
        let fade = (1.0 - progress) * (1.0 - progress) * smoothstep(radius, 0.6 * radius, distance);
        let strength = wave * fade;
        let direction = select(vec3<f32>(0.0), offset / distance, distance > 1e-6);
        result.position = result.position + direction * (0.04 * radius * strength);
        result.scale = result.scale * (1.0 + 0.5 * strength);
        result.emission = result.emission
            + gaussian_uniforms.effect_glow.rgb * (gaussian_uniforms.effect_glow.a * strength);
    }
    return result;
}

// After World Labs' homepage reveal. Ahead of a sphere growing from the
// origin on a cubic ease-in, splats are small pulsing points in their own
// colour, slowly drifting, with a hashed 15% culled. The sphere cuts them
// hard into full splats, tinting a band just inside its edge.
fn stars_stage(rest: vec3<f32>, hash: vec3<f32>, effect: SplatEffect) -> SplatEffect {
    var result = effect;
    let origin = gaussian_uniforms.effect_origin.xyz;
    let extent = max(gaussian_uniforms.effect_origin.w, 1e-4);
    let duration = max(gaussian_uniforms.stars_duration, 1e-4);
    let t = gaussian_uniforms.effect_time;
    let p = clamp(t / duration, 0.0, 1.0);
    let offset = rest - origin;
    let radial = effect_radial(offset);
    // The front starts just before the nearest splat and ends `band` past the
    // farthest, so the tint has left the scene when the stage drops out.
    let band = 0.06;
    let front = p * p * p * (1.0 + band + 0.01) - 0.01;
    let ahead = step(front, radial);
    let behind = 1.0 - ahead;
    let ring = (1.0 - smoothstep(0.0, band, front - radial)) * behind;
    // A smooth field of sines, so neighbouring points drift together.
    let q = offset / extent * 6.0;
    let phase = t * 1.1 + hash.x * 6.2831853;
    let drift = (sin(q.yzx * 2.0 + phase + vec3<f32>(0.0, 0.8, 1.6))
        + 0.5 * sin(q.zxy * 3.0 + t * vec3<f32>(1.3, 1.5, 1.7))) * (0.003 * extent);
    let pulse = 1.0 + 0.4 * sin(t * 2.2 + hash.y * 6.2831853 * 0.8);
    result.position = result.position + drift * ahead;
    result.point = mix(result.point, 1.0, ahead);
    result.point_size = mix(result.point_size, 0.0015 * extent * pulse, ahead);
    result.opacity = result.opacity * mix(1.0, step(0.15, hash.z), ahead);
    if ring > 0.0 {
        result.tint = vec4<f32>(gaussian_uniforms.effect_glow.rgb, max(result.tint.a, 0.8 * ring));
    }
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
    if (mask & EFFECT_SPREAD) != 0u {
        effect = spread_stage(rest, hash, effect);
    }
    if (mask & EFFECT_MAGIC) != 0u {
        effect = magic_stage(rest, hash, effect);
    }
    if (mask & EFFECT_REVEAL) != 0u {
        effect = reveal_stage(rest, hash, effect);
    }
    if (mask & EFFECT_STARS) != 0u {
        effect = stars_stage(rest, hash, effect);
    }
    if (mask & EFFECT_RIPPLE) != 0u {
        effect = ripple_stage(rest, hash, effect);
    }
    return effect;
}
