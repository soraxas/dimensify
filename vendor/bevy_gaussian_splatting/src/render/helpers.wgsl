#define_import_path bevy_gaussian_splatting::helpers

#import bevy_gaussian_splatting::bindings::{
    view,
    gaussian_uniforms,
}

fn cov2d(
    position: vec3<f32>,
    cov3d: array<f32, 6>,
) -> vec3<f32> {
    let Vrk = mat3x3(
        cov3d[0], cov3d[1], cov3d[2],
        cov3d[1], cov3d[3], cov3d[4],
        cov3d[2], cov3d[4], cov3d[5],
    );

    var t = view.view_from_world * vec4<f32>(position, 1.0);
    // Bevy's standard orthographic discriminator. Orthographic XY derivatives
    // are constant: neither depth nor off-axis position changes the footprint.
    // Substitute the Jacobian evaluation point, not the projected splat centre.
    if view.clip_from_view[3].w == 1.0 {
        t = vec4<f32>(0.0, 0.0, 1.0, 1.0);
    }

    let focal = vec2<f32>(
        view.clip_from_view[0].x * view.viewport.z,
        view.clip_from_view[1].y * view.viewport.w,
    );

    let s = 1.0 / (t.z * t.z);
#ifdef NORMALIZED_GAUSSIAN
    // The footprint is added in clip/NDC coordinates, not pixel-Y-down.
    // Preserve each projection axis sign (including a flipped render target).
    // A mixed-sign Jacobian mirrors the ellipse relative to its projected
    // center; OBB/AABB agreement alone cannot detect that shared error.
    let J = mat3x3(
        focal.x / t.z, 0.0, -(focal.x * t.x) * s,
        0.0, focal.y / t.z, -(focal.y * t.y) * s,
        0.0, 0.0, 0.0,
    );
#else
    let J = mat3x3(
        focal.x / t.z, 0.0, -(focal.x * t.x) * s,
        0.0, -focal.y / t.z, (focal.y * t.y) * s,
        0.0, 0.0, 0.0,
    );
#endif

    let W = transpose(
        mat3x3<f32>(
            view.view_from_world[0].xyz,
            view.view_from_world[1].xyz,
            view.view_from_world[2].xyz,
        )
    );

    let T = W * J;

    var cov = transpose(T) * transpose(Vrk) * T;
    cov[0][0] += 0.3f;
    cov[1][1] += 0.3f;

    return vec3<f32>(cov[0][0], cov[0][1], cov[1][1]);
}

// Bound the projected kernel, not the centre position. Perspective derivatives
// diverge beside the eye plane: without the reference viewport-relative cap,
// far-offscreen centres can paint a solid veil over the entire viewport.
// PlayCanvas 2.22.6 gsplatCorner caps each sqrt(8)-support axis at 2*vmin
// in these doubled-pixel coordinates. Preserve each profile's cutoff and cap
// covariance eigenvalues so AABB conics and OBB geometry use the same shape.
fn bound_kernel_covariance(cov: vec3<f32>, cutoff: f32) -> vec3<f32> {
    let vmin = min(1024.0, min(view.viewport.z, view.viewport.w));
    let max_sigma = 2.0 * vmin / cutoff;
    let limit = max_sigma * max_sigma;
    let mid = 0.5 * (cov.x + cov.z);
    let delta = 0.5 * (cov.x - cov.z);
    let radius = length(vec2<f32>(delta, cov.y));
    let major = mid + radius;
    if major <= limit {
        return cov;
    }
    if radius == 0.0 {
        return vec3<f32>(limit, 0.0, limit);
    }
    let minor = min(max(mid - radius, 0.1), limit);
    let gap = limit - minor;
    // Spectral projector of the major eigenvector; avoids normalizing a
    // cancelling eigenvector and preserves the original covariance sign.
    let projector = 0.5 * vec3<f32>(1.0 + delta / radius, cov.y / radius, 1.0 - delta / radius);
    return vec3<f32>(minor, 0.0, minor) + gap * projector;
}

fn get_bounding_box_clip(
    cov2d: vec3<f32>,
    direction: vec2<f32>,
    cutoff: f32,
) -> vec4<f32> {
    // return vec4<f32>(offset, uv);

    let det = cov2d.x * cov2d.z - cov2d.y * cov2d.y;
    let trace = cov2d.x + cov2d.z;
    let mid = 0.5 * trace;
    let discriminant = max(0.0, mid * mid - det);

    let term = sqrt(discriminant);

    let lambda1 = mid + term;
    let lambda2 = max(mid - term, 0.0);

    let x_axis_length = sqrt(lambda1);
    let y_axis_length = sqrt(lambda2);

#ifdef USE_AABB
    let radius_px = cutoff * max(x_axis_length, y_axis_length);
    let radius_ndc = vec2<f32>(
        radius_px / view.viewport.zw,
    );

    return vec4<f32>(
        radius_ndc * direction,
        radius_px * direction,
    );
#endif

#ifdef USE_OBB

    let a = (cov2d.x - cov2d.z) * (cov2d.x - cov2d.z);
    let b = sqrt(a + 4.0 * cov2d.y * cov2d.y);
    let major_radius = sqrt((cov2d.x + cov2d.z + b) * 0.5);
    let minor_radius = sqrt((cov2d.x + cov2d.z - b) * 0.5);

    let bounds = cutoff * vec2<f32>(
        major_radius,
        minor_radius,
    );

    // Preserve each profile's covariance convention, but compute a stable major
    // axis for both. The old Legacy ( -xy, lambda1-xx ) vector cancels when X is
    // the major axis: roundoff can rotate a horizontal ellipse by 90 degrees,
    // or normalize(0) can collapse a valid isotropic footprint entirely.
    var off_diagonal = cov2d.y;
#ifndef NORMALIZED_GAUSSIAN
    off_diagonal = -off_diagonal;
#endif
    var eigvec1 = vec2<f32>(1.0, 0.0);
    if off_diagonal == 0.0 {
        if cov2d.z > cov2d.x {
            eigvec1 = vec2<f32>(0.0, 1.0);
        }
    } else {
        var axis = vec2<f32>(lambda1 - cov2d.z, off_diagonal);
        if cov2d.z > cov2d.x {
            axis = vec2<f32>(off_diagonal, lambda1 - cov2d.x);
        }
        // Choose the non-cancelling diagonal difference, then rescale before
        // normalizing to avoid underflow for very small valid covariance entries.
        axis /= max(abs(axis.x), abs(axis.y));
        eigvec1 = normalize(axis);
    }
#ifndef NORMALIZED_GAUSSIAN
    // Keep Legacy's nonnegative-Y eigenvector/corner convention wherever it
    // was well-defined. Reversing both axes draws the same ellipse, but need
    // not change existing corner/UV correspondence to repair singular axes.
    if eigvec1.y < 0.0 {
        eigvec1 = -eigvec1;
    }
#endif
    let eigvec2 = vec2<f32>(
        eigvec1.y,
        -eigvec1.x
    );

    let rotation_matrix = transpose(
        mat2x2(
            eigvec1,
            eigvec2,
        )
    );

    let scaled_vertex = direction * bounds;
    let rotated_vertex = scaled_vertex * rotation_matrix;

    let scaling_factor = 1.0 / view.viewport.zw;
    let ndc_vertex = rotated_vertex * scaling_factor;

    return vec4<f32>(
        ndc_vertex,
        rotated_vertex,
    );
#endif
}

fn intrinsic_matrix() -> mat3x4<f32> {
    let focal = vec2<f32>(
        view.clip_from_view[0].x * view.viewport.z / 2.0,
        view.clip_from_view[1].y * view.viewport.w / 2.0,
    );

    let Ks = mat3x4<f32>(
        vec4<f32>(focal.x, 0.0, 0.0, (view.viewport.z - 1.0) / 2.0),
        vec4<f32>(0.0, focal.y, 0.0, (view.viewport.w - 1.0) / 2.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0)
    );

    return Ks;
}

fn get_rotation_matrix(
    rotation: vec4<f32>,
) -> mat3x3<f32> {
    let r = rotation.x;
    let x = rotation.y;
    let y = rotation.z;
    let z = rotation.w;

    return mat3x3<f32>(
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - r * z),
        2.0 * (x * z + r * y),

        2.0 * (x * y + r * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - r * x),

        2.0 * (x * z - r * y),
        2.0 * (y * z + r * x),
        1.0 - 2.0 * (x * x + y * y),
    );
}

fn get_scale_matrix(
    scale: vec3<f32>,
) -> mat3x3<f32> {
    return mat3x3<f32>(
        scale.x * gaussian_uniforms.global_scale, 0.0, 0.0,
        0.0, scale.y * gaussian_uniforms.global_scale, 0.0,
        0.0, 0.0, scale.z * gaussian_uniforms.global_scale,
    );
}
