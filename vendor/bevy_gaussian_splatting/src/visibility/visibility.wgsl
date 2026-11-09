#define_import_path bevy_gaussian_splatting::visibility

#import bevy_gaussian_splatting::bindings::gaussian_uniforms

// Display modes (CloudUniform.vis_mode); 0 disables the field entirely.
const VIS_OFF: u32 = 0u;
const VIS_FADED: u32 = 1u;
const VIS_REMOVED: u32 = 2u;
const VIS_UNCERTAINTY: u32 = 3u;
const VIS_OVERLAY: u32 = 4u;
// Curves (CloudUniform.vis_curve).
const CURVE_HIDE: u32 = 0u;
const CURVE_SMOOTH: u32 = 1u;
const CURVE_LINEAR: u32 = 2u;
const KEEP_EPSILON: f32 = 1.0 / 255.0;
const FIELD_WORDS: u32 = 14u;

fn visibility_enabled() -> bool {
    return gaussian_uniforms.vis_mode != VIS_OFF;
}

// gsplat real SH basis, degree 4 (Sloan 2013), dotted with the stored coefficients.
// `w` holds 28 halves: SH 0..24, v_peak, normalised fit residual, padding.
fn visibility_evidence(w: array<u32, 14>, d: vec3<f32>) -> vec3<f32> {
    var h: array<f32, 28>;
    for (var i = 0u; i < 14u; i += 1u) {
        let p = unpack2x16float(w[i]);
        h[2u * i] = p.x;
        h[2u * i + 1u] = p.y;
    }
    let x = d.x;
    let y = d.y;
    let z = d.z;
    var v = 0.2820947917738781 * h[0];
    let a1 = -0.48860251190292;
    v += a1 * y * h[1] - a1 * z * h[2] + a1 * x * h[3];
    let z2 = z * z;
    let b2 = -1.092548430592079 * z;
    let a2 = 0.5462742152960395;
    let c1 = x * x - y * y;
    let s1 = 2.0 * x * y;
    let p20 = 0.9461746957575601 * z2 - 0.3153915652525201;
    v += a2 * s1 * h[4] + b2 * y * h[5] + p20 * h[6] + b2 * x * h[7] + a2 * c1 * h[8];
    let c3 = -2.285228997322329 * z2 + 0.4570457994644658;
    let b3 = 1.445305721320277 * z;
    let a3 = -0.5900435899266435;
    let c2 = x * c1 - y * s1;
    let s2 = x * s1 + y * c1;
    let p30 = z * (1.865881662950577 * z2 - 1.119528997770346);
    v += a3 * s2 * h[9] + b3 * s1 * h[10] + c3 * y * h[11] + p30 * h[12] + c3 * x * h[13] + b3 * c1 * h[14] + a3 * c2 * h[15];
    let d4 = z * (-4.683325804901025 * z2 + 2.007139630671868);
    let c4 = 3.31161143515146 * z2 - 0.47308734787878;
    let b4 = -1.770130769779931 * z;
    let a4 = 0.6258357354491763;
    let cc3 = x * c2 - y * s2;
    let ss3 = x * s2 + y * c2;
    let p40 = 1.984313483298443 * z2 * (1.865881662950577 * z2 - 1.119528997770346) - 1.006230589874905 * p20;
    v += a4 * ss3 * h[16] + b4 * s2 * h[17] + c4 * s1 * h[18] + d4 * y * h[19] + p40 * h[20]
        + d4 * x * h[21] + c4 * c1 * h[22] + b4 * c2 * h[23] + a4 * cc3 * h[24];
    return vec3<f32>(v, h[25], h[26]);
}

// Query rule: (1 - exp(-gain v)) * clamp(v / (rho v_peak), 0, 1), capped at 1 - fit_strength * fit.
fn visibility_query(w: array<u32, 14>, d: vec3<f32>) -> f32 {
    let e = visibility_evidence(w, d);
    let v = max(e.x, 0.0);
    let rel = clamp(v / max(e.y, 1e-6) / max(gaussian_uniforms.vis_rho, 1e-6), 0.0, 1.0);
    return min((1.0 - exp(-gaussian_uniforms.vis_gain * v)) * rel, 1.0 - gaussian_uniforms.vis_fit * e.z);
}

// Opacity multiplier for visibility `vis` (matches VisibilitySettings::keep).
fn visibility_keep(vis: f32) -> f32 {
    let lo = min(gaussian_uniforms.vis_lo, gaussian_uniforms.vis_hi);
    let hi = max(gaussian_uniforms.vis_lo, gaussian_uniforms.vis_hi);
    if gaussian_uniforms.vis_curve == CURVE_HIDE {
        return select(0.0, 1.0, vis >= 0.5 * (lo + hi));
    }
    if hi - lo < 1e-6 {
        return select(0.0, 1.0, vis >= lo);
    }
    let u = clamp((vis - lo) / (hi - lo), 0.0, 1.0);
    var s = u;
    if gaussian_uniforms.vis_curve == CURVE_SMOOTH {
        s = u * u * (3.0 - 2.0 * u);
    }
    return pow(s, max(gaussian_uniforms.vis_gamma, 1e-3));
}

// Opacity multiplier the current display mode applies (1 for the colour modes).
fn visibility_opacity(keep: f32) -> f32 {
    let mode = gaussian_uniforms.vis_mode;
    if mode == VIS_FADED {
        return keep;
    }
    if mode == VIS_REMOVED {
        return 1.0 - keep;
    }
    return 1.0;
}

// Whether the sort may skip a Gaussian entirely.
fn visibility_culled(keep: f32) -> bool {
    return visibility_opacity(keep) < KEEP_EPSILON;
}

// Local-frame unit direction from the camera to a Gaussian (the field's frame).
fn visibility_direction(world_delta: vec3<f32>) -> vec3<f32> {
    let t = gaussian_uniforms.transform;
    let local = vec3<f32>(
        dot(normalize(t[0].xyz), world_delta),
        dot(normalize(t[1].xyz), world_delta),
        dot(normalize(t[2].xyz), world_delta),
    );
    return normalize(local);
}

// Turbo colour map (Mikhailov 2019, polynomial fit), x in [0, 1].
fn turbo(x_in: f32) -> vec3<f32> {
    let x = clamp(x_in, 0.0, 1.0);
    let v4 = vec4<f32>(1.0, x, x * x, x * x * x);
    let v2 = v4.zw * v4.z;
    let r = dot(v4, vec4<f32>(0.13572138, 4.61539260, -42.66032258, 132.13108234)) + dot(v2, vec2<f32>(-152.94239396, 59.28637943));
    let g = dot(v4, vec4<f32>(0.09140261, 2.19418839, 4.84296658, -14.18503333)) + dot(v2, vec2<f32>(4.27729857, 2.82956604));
    let b = dot(v4, vec4<f32>(0.10667330, 12.64194608, -60.58204836, 110.36276771)) + dot(v2, vec2<f32>(-89.90310912, 27.34824973));
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Colour the current display mode shows, given the regular colour and visibility.
fn visibility_color(rgb: vec3<f32>, vis: f32) -> vec3<f32> {
    let mode = gaussian_uniforms.vis_mode;
    if mode == VIS_UNCERTAINTY {
        return turbo(1.0 - vis);
    }
    if mode == VIS_OVERLAY {
        return mix(rgb, turbo(1.0 - vis), clamp(gaussian_uniforms.vis_overlay * (1.0 - vis), 0.0, 1.0));
    }
    return rgb;
}
