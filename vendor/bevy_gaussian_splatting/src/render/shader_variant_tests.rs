//! Compose every pipeline entry shader under the shader-def combinations the
//! crate's features and pipeline keys can produce, and check every WESL `@if`
//! flag is one Rust can set. Unset WESL flags silently evaluate to false, so a
//! renamed or misspelled flag compiles but selects the wrong branch.
use super::*;
use bevy::shader::{ShaderCache, ShaderCacheError, ShaderCacheSource};
use std::collections::BTreeSet;

/// Defs selected by Cargo features rather than by the pipeline key.
const FEATURE_DEFS: &[&str] = &[
    "PACKED",
    "BUFFER_STORAGE",
    "BUFFER_TEXTURE",
    "F32",
    "PACKED_F32",
    "PLANAR_F32",
    "PLANAR_TEXTURE_F32",
    "PRECOMPUTE_COVARIANCE_3D",
    "WEBGL2",
    "READ_WRITE_POINTS",
];

/// Storage def sets, mirroring the `cfg` logic in `shader_defs_with_defines`.
const STORAGE: &[(&str, &[&'static str])] = &[
    ("planar", &["BUFFER_STORAGE", "F32", "PLANAR_F32"]),
    (
        "packed",
        &[
            "PACKED",
            "BUFFER_STORAGE",
            "F32",
            "PACKED_F32",
            "PLANAR_F32",
        ],
    ),
    ("texture", &["BUFFER_TEXTURE", "F32", "PLANAR_TEXTURE_F32"]),
    (
        "packed_texture",
        &[
            "PACKED",
            "BUFFER_TEXTURE",
            "F32",
            "PACKED_F32",
            "PLANAR_TEXTURE_F32",
        ],
    ),
];

const fn pad_4(n: u32) -> u32 {
    n.div_ceil(4) * 4
}

/// Replace the compiled SH degree's defs with those of `degree`.
fn with_sh_degree(defs: &mut [ShaderDefVal], degree: u32) {
    let per_channel = (degree + 1).pow(2);
    let coeffs = pad_4(per_channel * 3);
    let time = SH_4D_DEGREE_TIME as u32;
    for def in defs {
        match def {
            ShaderDefVal::Bool(name, value) => {
                *value = match name.as_ref() {
                    "SINGLE_SH_VEC4_PLANE" => coeffs / 4 == 1,
                    "SH_COEFF_COUNT_GT_11" => coeffs > 11,
                    "SH_COEFF_COUNT_GT_26" => coeffs > 26,
                    "SH_COEFF_COUNT_GT_47" => coeffs > 47,
                    "SH_DEGREE_GT_0" => degree > 0,
                    "SH_DEGREE_GT_1" => degree > 1,
                    "SH_DEGREE_GT_2" => degree > 2,
                    _ => *value,
                }
            }
            ShaderDefVal::UInt(name, value) => {
                *value = match name.as_ref() {
                    "SH_COEFF_COUNT" => coeffs,
                    "SH_4D_COEFF_COUNT" => pad_4(per_channel * (time + 1) * 3),
                    "SH_DEGREE" => degree,
                    "HALF_SH_COEFF_COUNT" => coeffs / 2,
                    "SH_VEC4_PLANES" => coeffs / 4,
                    _ => *value,
                }
            }
            ShaderDefVal::Int(..) => {}
        }
    }
}

fn def_name(def: &ShaderDefVal) -> &str {
    match def {
        ShaderDefVal::Bool(name, _) | ShaderDefVal::Int(name, _) | ShaderDefVal::UInt(name, _) => {
            name
        }
    }
}

/// A variant: the pipeline defs with Cargo-feature defs replaced.
fn variant(
    key: CloudPipelineKey,
    storage: &[&'static str],
    extra: &[&'static str],
    degree: u32,
) -> Vec<ShaderDefVal> {
    let mut defs: Vec<_> = shader_defs(key)
        .into_iter()
        .filter(|def| !FEATURE_DEFS.contains(&def_name(def)))
        .collect();
    defs.extend(storage.iter().chain(extra).map(|&name| name.into()));
    with_sh_degree(&mut defs, degree);
    defs
}

/// Pipeline keys: a base per mode/profile/rasterizer plus one toggle at a time.
fn keys() -> Vec<(String, CloudPipelineKey)> {
    let mut keys = Vec::new();
    for mode in [
        GaussianMode::Gaussian2d,
        GaussianMode::Gaussian3d,
        GaussianMode::Gaussian4d,
    ] {
        for profile in [
            GaussianRasterProfile::Legacy,
            GaussianRasterProfile::NormalizedGaussian,
        ] {
            for rasterize in [
                RasterizeMode::Classification,
                RasterizeMode::Color,
                RasterizeMode::Depth,
                RasterizeMode::Normal,
                RasterizeMode::OpticalFlow,
                RasterizeMode::Position,
                RasterizeMode::Velocity,
            ] {
                let base = CloudPipelineKey {
                    gaussian_mode: mode,
                    raster_profile: profile,
                    rasterize_mode: rasterize,
                    ..default()
                };
                keys.push((format!("{mode:?}/{profile:?}/{rasterize:?}"), base));
            }
            let base = CloudPipelineKey {
                gaussian_mode: mode,
                raster_profile: profile,
                ..default()
            };
            let toggles: [(&str, CloudPipelineKey); 14] = [
                ("aabb", CloudPipelineKey { aabb: true, ..base }),
                (
                    "binary_op",
                    CloudPipelineKey {
                        binary_gaussian_op: true,
                        ..base
                    },
                ),
                (
                    "bbox",
                    CloudPipelineKey {
                        visualize_bounding_box: true,
                        ..base
                    },
                ),
                (
                    "adaptive",
                    CloudPipelineKey {
                        opacity_adaptive_radius: true,
                        ..base
                    },
                ),
                (
                    "selected",
                    CloudPipelineKey {
                        draw_mode: DrawMode::Selected,
                        ..base
                    },
                ),
                (
                    "highlight",
                    CloudPipelineKey {
                        draw_mode: DrawMode::HighlightSelected,
                        ..base
                    },
                ),
                (
                    "stochastic",
                    CloudPipelineKey {
                        stochastic: true,
                        ..base
                    },
                ),
                (
                    "depth_pass",
                    CloudPipelineKey {
                        depth_pass: true,
                        depth_pass_alpha: 0.5f32.to_bits(),
                        ..base
                    },
                ),
                (
                    "srgb",
                    CloudPipelineKey {
                        output_srgb: true,
                        ..base
                    },
                ),
                (
                    "compact",
                    CloudPipelineKey {
                        compact_work: true,
                        ..base
                    },
                ),
                (
                    "projected",
                    CloudPipelineKey {
                        compact_work: true,
                        projected_work: true,
                        ..base
                    },
                ),
                ("effects", CloudPipelineKey { effects: true, ..base }),
                (
                    "effects_compact",
                    CloudPipelineKey {
                        effects: true,
                        compact_work: true,
                        ..base
                    },
                ),
                (
                    "effects_stochastic_depth",
                    CloudPipelineKey {
                        effects: true,
                        stochastic: true,
                        depth_pass: true,
                        depth_pass_alpha: 0.5f32.to_bits(),
                        ..base
                    },
                ),
            ];
            for (name, key) in toggles {
                keys.push((format!("{mode:?}/{profile:?}/{name}"), key));
            }
        }
    }
    keys
}

/// (entry module path, label, defs) for every swept variant.
fn variants() -> Vec<(&'static str, String, Vec<ShaderDefVal>)> {
    let mut out = Vec::new();
    let mut push = |entry, label: String, defs| out.push((entry, label, defs));
    for &(storage_name, storage) in STORAGE {
        for (extra_name, extra) in [
            ("", &[][..]),
            ("+webgl2", &["WEBGL2"][..]),
            ("+precompute", &["PRECOMPUTE_COVARIANCE_3D"][..]),
            ("+rw_points", &["READ_WRITE_POINTS"][..]),
        ] {
            for degree in [0, 1, 2, 3, 4] {
                let tag = format!("{storage_name}{extra_name}/sh{degree}");
                for (key_name, key) in keys() {
                    // The full key grid at the default degree; modes/profiles elsewhere.
                    if degree != 3 && key_name.split('/').nth(2) != Some("Color") {
                        continue;
                    }
                    push(
                        "render/gaussian.wesl",
                        format!("{tag}/{key_name}"),
                        variant(key, storage, extra, degree),
                    );
                }
                for (name, key) in [
                    ("default", CloudPipelineKey::default()),
                    (
                        "compact",
                        CloudPipelineKey {
                            compact_work: true,
                            ..default()
                        },
                    ),
                    (
                        "projected",
                        CloudPipelineKey {
                            compact_work: true,
                            projected_work: true,
                            ..default()
                        },
                    ),
                ] {
                    push(
                        "sort/radix.wesl",
                        format!("{tag}/{name}"),
                        variant(key, storage, extra, degree),
                    );
                }
                let default = CloudPipelineKey::default();
                push(
                    "sort/temporal.wesl",
                    tag.clone(),
                    variant(default, storage, extra, degree),
                );
                push(
                    "morph/particle.wesl",
                    tag.clone(),
                    variant(default, storage, extra, degree),
                );
                push(
                    "morph/interpolate.wesl",
                    tag.clone(),
                    variant(
                        CloudPipelineKey {
                            binary_gaussian_op: true,
                            ..default
                        },
                        storage,
                        extra,
                        degree,
                    ),
                );
                push(
                    "render/compositing_transfer.wesl",
                    tag.clone(),
                    variant(default, storage, extra, degree),
                );
                push(
                    "render/projected.wesl",
                    tag.clone(),
                    variant(
                        CloudPipelineKey {
                            compact_work: true,
                            raster_profile: GaussianRasterProfile::NormalizedGaussian,
                            ..default
                        },
                        storage,
                        extra,
                        degree,
                    ),
                );
            }
        }
    }
    // The WebGL fallback receives only its own defs.
    push("render/webgl/shader.wesl", "legacy".into(), vec![]);
    push(
        "render/webgl/shader.wesl",
        "normalized".into(),
        vec!["NORMALIZED_GAUSSIAN".into()],
    );
    out
}

fn validate(
    _: &(),
    source: ShaderCacheSource,
    _: &bevy::shader::ValidateShader,
) -> Result<(), ShaderCacheError> {
    let ShaderCacheSource::Wgsl(source) = source else {
        return Err(ShaderCacheError::CreateShaderModule("expected WGSL".into()));
    };
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .map_err(|error| ShaderCacheError::CreateShaderModule(error.emit_to_string(&source)))?;
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|error| ShaderCacheError::CreateShaderModule(format!("{error:?}")))?;
    Ok(())
}

/// Writes one `ok`/`FAIL` line per variant to `SHADER_VARIANT_REPORT` (default:
/// the temp directory) so two trees can be compared; the failure set, not its
/// absence, is the contract.
#[test]
#[ignore = "sweeps all shader variants; SHADER_VARIANT_REPORT selects the report file"]
fn shader_variant_sweep() {
    let report = std::env::var_os("SHADER_VARIANT_REPORT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("shader-variants.txt"));
    let assets = super::raster_profile_tests::shader_assets();
    let mut ids = std::collections::HashMap::new();
    let mut cache = ShaderCache::<(), ()>::new((), validate);
    for (id, shader) in assets.iter() {
        if let Some(path) = shader
            .path
            .strip_prefix("embedded://bevy_gaussian_splatting/")
        {
            ids.insert(path.to_owned(), id);
        }
        cache.set_shader(id, shader.clone());
    }
    let mut lines = Vec::new();
    let mut failures = 0;
    for (entry, label, defs) in variants() {
        let result = cache.get(0, ids[entry], &defs);
        let line = match result {
            Ok(_) => format!("ok   {entry} {label}"),
            Err(error) => {
                failures += 1;
                let first = error
                    .to_string()
                    .lines()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("FAIL {entry} {label} :: {first}")
            }
        };
        lines.push(line);
    }
    lines.sort();
    std::fs::write(&report, lines.join("\n") + "\n").unwrap();
    eprintln!("{} variants, {failures} failures: {}", lines.len(), report.display());
}

/// Identifiers tested by `@if`/`@elif` in every vendor WESL module.
fn wesl_flags() -> BTreeSet<(String, String)> {
    let mut flags = BTreeSet::new();
    let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            continue;
        }
        if path.extension().is_none_or(|e| e != "wesl") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        for start in source
            .match_indices("@if(")
            .chain(source.match_indices("@elif("))
        {
            let open = start.0 + start.1.len();
            let mut depth = 1;
            let close = source[open..]
                .char_indices()
                .find(|&(_, c)| {
                    depth += match c {
                        '(' => 1,
                        ')' => -1,
                        _ => 0,
                    };
                    depth == 0
                })
                .map(|(i, _)| open + i)
                .unwrap();
            for word in source[open..close].split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if word.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                    flags.insert((word.to_owned(), file.clone()));
                }
            }
        }
    }
    flags
}

#[test]
fn every_wesl_flag_can_be_set_by_rust() {
    let settable: BTreeSet<String> = variants()
        .iter()
        .flat_map(|(_, _, defs)| defs.iter().map(|def| def_name(def).to_owned()))
        .chain(FEATURE_DEFS.iter().map(|&name| name.to_owned()))
        // f16 storage is disabled in `shader_defs_with_defines`; its paths are dead.
        .chain(["PLANAR_F16".into(), "PLANAR_TEXTURE_F16".into()])
        .collect();
    let unknown: Vec<_> = wesl_flags()
        .into_iter()
        .filter(|(flag, _)| !settable.contains(flag) && flag != "true" && flag != "false")
        .collect();
    assert!(unknown.is_empty(), "flags no Rust def sets: {unknown:?}");
}
