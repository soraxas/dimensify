//! Compose the actual render shader and execute its shared production raster functions.
use super::*;
use bevy::shader::{ShaderCache, ShaderCacheSource};
use std::path::Path;

fn load_shader_tree(path: &Path, assets: &mut Assets<Shader>, handles: &mut Vec<Handle<Shader>>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            load_shader_tree(&path, assets, handles);
        } else if path.extension().is_some_and(|e| e == "wgsl") {
            let source = std::fs::read_to_string(&path).unwrap();
            if source.contains("#define_import_path") {
                handles.push(assets.add(Shader::from_wgsl(
                    source,
                    path.to_string_lossy().into_owned(),
                )));
            }
        }
    }
}

pub(super) fn production_module(key: CloudPipelineKey, probe: bool) -> wgpu::naga::Module {
    let mut source = include_str!("gaussian.wgsl").to_owned();
    if probe {
        source.push_str(PROBE);
    }
    production_module_source(key, source)
}

pub(super) fn production_module_source(
    key: CloudPipelineKey,
    source: String,
) -> wgpu::naga::Module {
    let mut assets = Assets::<Shader>::default();
    let mut handles = Vec::new();
    load_shader_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut assets,
        &mut handles,
    );
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(&std::env::var_os("HOME").unwrap()).join(".cargo"));
    let mut found = [false; 3];
    for registry in std::fs::read_dir(cargo_home.join("registry/src")).unwrap() {
        let registry = registry.unwrap().path();
        for (i, name) in [
            "bevy_render-0.19.1",
            "bevy_pbr-0.19.1",
            "bevy_core_pipeline-0.19.1",
        ]
        .into_iter()
        .enumerate()
        {
            let src = registry.join(name).join("src");
            if src.exists() {
                load_shader_tree(&src, &mut assets, &mut handles);
                found[i] = true;
            }
        }
    }
    assert!(
        found.into_iter().all(|v| v),
        "resolved Bevy 0.19.1 registry shader sources are required"
    );
    assets
        .insert(
            GAUSSIAN_SHADER_HANDLE.id(),
            Shader::from_wgsl(source, "gaussian.wgsl"),
        )
        .unwrap();
    let mut cache = ShaderCache::new(
        (),
        wgpu::Features::empty(),
        wgpu::DownlevelFlags::all(),
        |_, source, _| {
            let ShaderCacheSource::Naga(module) = source else {
                panic!("expected Naga")
            };
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
            Ok(module)
        },
    );
    for (id, shader) in assets.iter() {
        cache.set_shader(id, shader.clone());
    }
    (*cache
        .get(0, GAUSSIAN_SHADER_HANDLE.id(), &shader_defs(key))
        .unwrap())
    .clone()
}

#[cfg(all(feature = "planar", feature = "buffer_storage", feature = "sort_radix"))]
#[test]
fn projected_records_keep_sort_keys_integer_in_browser_wgsl() {
    // Native backends consume Naga directly. WebGPU serializes it back to WGSL:
    // a float-bitcast invalid-key sentinel is a non-finite constant expression
    // that Dawn rejects even though native validation/rendering succeeds.
    let key = CloudPipelineKey {
        compact_work: true,
        raster_profile: GaussianRasterProfile::NormalizedGaussian,
        ..default()
    };
    let modules = [
        production_module_source(key, include_str!("projected.wgsl").into()),
        production_module(
            CloudPipelineKey {
                projected_work: true,
                ..key
            },
            false,
        ),
    ];
    for module in modules {
        // Naga also accepts some non-finite constant expressions Dawn rejects.
        // Lock down the integer wire format, not only Naga's own roundtrip.
        let (members, span) = module
            .types
            .iter()
            .find_map(|(_, ty)| {
                if let wgpu::naga::TypeInner::Struct { members, span } = &ty.inner {
                    if members
                        .first()
                        .is_some_and(|m| m.name.as_deref() == Some("centre_key"))
                    {
                        return Some((members, *span));
                    }
                }
                None
            })
            .expect("projection record present");
        assert_eq!(span, 32, "projection reservation/layout contract");
        assert!(
            matches!(
                module.types[members[0].ty].inner,
                wgpu::naga::TypeInner::Vector {
                    size: wgpu::naga::VectorSize::Quad,
                    scalar: wgpu::naga::Scalar {
                        kind: wgpu::naga::ScalarKind::Uint,
                        width: 4
                    },
                }
            ),
            "depth-key words must stay integer, including the invalid sentinel"
        );
        let info = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("native module validates");
        let browser_source = wgpu::naga::back::wgsl::write_string(
            &module,
            &info,
            wgpu::naga::back::wgsl::WriterFlags::empty(),
        )
        .expect("browser WGSL is emitted");
        let roundtrip = wgpu::naga::front::wgsl::parse_str(&browser_source)
            .expect("generated browser WGSL parses without NaN constants");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&roundtrip)
        .expect("browser roundtrip validates");
    }
}

// This wrapper only supplies inputs/readback; all footprint, exponent and alpha math is production.
const PROBE: &str = r#"
#import bevy_gaussian_splatting::helpers::cov2d
@group(1) @binding(1) var<storage, read_write> raster_results: array<vec4<f32>>;
@compute @workgroup_size(1)
fn raster_probe(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    let radii = array<f32, 6>(0.0, 0.25, 1.0, 1.25, 0.25, 0.0);
    let opacities = array<f32, 6>(0.1, 0.1, 0.1, 0.1, 1.0, 1.0);
    let uv = vec2<f32>(sqrt(radii[i]), 0.0);
    let power = raster_obb_power(uv);
    // Covariance [2,1;1,2], inverse [2/3,-1/3;-1/3,2/3].
    // d=(sqrt(3),sqrt(3)) gives power=-1 in the normalized branch.
    let conic = vec3<f32>(2.0/3.0, -1.0/3.0, 2.0/3.0);
    let aabb_power = raster_aabb_power(vec2<f32>(sqrt(3.0)), conic);
    raster_results[2u * i] = vec4<f32>(raster_alpha(power, opacities[i]), raster_cutoff(opacities[i]), power, aabb_power);
    raster_results[2u * i + 1u] = get_bounding_box_clip(vec3<f32>(2.0, 1.0, 2.0), vec2<f32>(1.0), raster_cutoff(opacities[i]));
    if i == 0u {
        // World covariance: a +45-degree XY ellipse with eigenvalues 3,1
        // and Z variance 1. The actual Bevy perspective matrix and identity
        // view project its positive XY correlation to positive NDC XY.
        let world_cov = array<f32, 6>(2.0, 1.0, 0.0, 2.0, 0.0, 1.0);
        let projected = cov2d(vec3<f32>(0.0, 0.0, -100.0), world_cov);
        let axis = get_bounding_box_clip(projected, vec2<f32>(1.0, 0.0), raster_cutoff(0.1));
        raster_results[19] = vec4<f32>(projected, axis.x * axis.y);
        raster_results[20] = vec4<f32>(cov2d(vec3<f32>(20.0, 10.0, -100.0), world_cov), 0.0);
        raster_results[12] = vec4<f32>(
            select(0.0, 1.0, raster_is_clipped(0.0, 0.003)),
            select(0.0, 1.0, raster_is_clipped(0.0, 0.004)),
            select(0.0, 1.0, raster_is_clipped(-4.0, 0.5)),
            select(0.0, 1.0, raster_is_clipped(-4.01, 0.5))
        );
    }
#ifdef USE_OBB
    let covariances = array<vec3<f32>, 6>(
        vec3<f32>(2.0, 1.0, 2.0), vec3<f32>(2.0, -1.0, 2.0),
        vec3<f32>(4.0, 0.0, 1.0), vec3<f32>(1.0, 0.0, 4.0),
        vec3<f32>(2.0, 0.0, 2.0), vec3<f32>(4.0, 1.0, 2.0)
    );
    let displacements = array<vec2<f32>, 6>(
        vec2<f32>(sqrt(3.0)), vec2<f32>(sqrt(3.0)),
        vec2<f32>(2.0, 0.0), vec2<f32>(0.0, 2.0),
        vec2<f32>(1.0), vec2<f32>(1.0, 2.0)
    );
    let cov = covariances[i];
    let d = displacements[i];
    // Recover the same physical point's UV from the actual production quad axes.
    let major = get_bounding_box_clip(cov, vec2<f32>(1.0, 0.0), raster_cutoff(0.1)).zw;
    let minor = get_bounding_box_clip(cov, vec2<f32>(0.0, 1.0), raster_cutoff(0.1)).zw;
    let point_uv = vec2<f32>(dot(d, major) / dot(major, major), dot(d, minor) / dot(minor, minor));
    let point_obb_power = raster_obb_power(point_uv);
    let det = cov.x * cov.z - cov.y * cov.y;
    let inverse_cov = vec3<f32>(cov.z, -cov.y, cov.x) / det;
    let point_aabb_power = raster_aabb_power(d, inverse_cov);
    raster_results[13u + i] = vec4<f32>(point_obb_power, point_aabb_power,
        raster_alpha(point_obb_power, 0.1), raster_alpha(point_aabb_power, 0.1));
#endif
}
"#;

#[test]
fn compact_raster_uses_integer_work_planes_not_expanded_sh() {
    let module = production_module(
        CloudPipelineKey {
            compact_work: true,
            ..default()
        },
        false,
    );
    let planes: Vec<_> = module
        .global_variables
        .iter()
        .filter_map(|(_, v)| {
            v.binding
                .as_ref()
                .filter(|b| b.group == 2)
                .map(|b| b.binding)
        })
        .collect();
    assert_eq!(
        planes.len(),
        3,
        "compact draw must not retain a full SH plane"
    );
    for (_, variable) in module.global_variables.iter() {
        if variable.binding.as_ref().is_some_and(|b| b.group == 2) {
            let wgpu::naga::TypeInner::Array { base, .. } = module.types[variable.ty].inner else {
                panic!("storage array")
            };
            let scalar = match module.types[base].inner {
                wgpu::naga::TypeInner::Vector { scalar, .. }
                | wgpu::naga::TypeInner::Scalar(scalar) => scalar,
                _ => panic!("compact integer elements"),
            };
            assert_eq!(scalar.kind, wgpu::naga::ScalarKind::Uint);
        }
    }
}

#[test]
fn production_raster_shader_validates_profile_and_geometry_variants() {
    for gaussian_mode in [
        GaussianMode::Gaussian2d,
        GaussianMode::Gaussian3d,
        GaussianMode::Gaussian4d,
    ] {
        for raster_profile in [
            GaussianRasterProfile::Legacy,
            GaussianRasterProfile::NormalizedGaussian,
        ] {
            for aabb in [false, true] {
                let key = CloudPipelineKey {
                    gaussian_mode,
                    raster_profile,
                    aabb,
                    opacity_adaptive_radius: true,
                    ..default()
                };
                let normalized =
                    shader_defs(key).contains(&ShaderDefVal::from("NORMALIZED_GAUSSIAN"));
                assert_eq!(
                    normalized,
                    gaussian_mode == GaussianMode::Gaussian3d
                        && raster_profile == GaussianRasterProfile::NormalizedGaussian
                );
                for output_srgb in [false, true] {
                    let module = production_module(CloudPipelineKey { output_srgb, ..key }, false);
                    assert!(module.entry_points.iter().any(|p| p.name == "vs_points"));
                    assert!(module.entry_points.iter().any(|p| p.name == "fs_main"));
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn production_raster_gpu_numerical_readback() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    eprintln!(
        "adapter={:?}; limits={:?}; features={:?}",
        resources.2.0,
        resources.0.limits(),
        resources.0.features()
    );
    let device = resources.0;
    let queue = resources.1;
    let raw = device.wgpu_device();
    for raster_profile in [
        GaussianRasterProfile::Legacy,
        GaussianRasterProfile::NormalizedGaussian,
    ] {
        for adaptive in [false, true] {
            for aabb in [false, true] {
                let module = production_module(
                    CloudPipelineKey {
                        raster_profile,
                        aabb,
                        opacity_adaptive_radius: adaptive,
                        ..default()
                    },
                    true,
                );
                let view_global = module
                    .global_variables
                    .iter()
                    .map(|(_, v)| v)
                    .find(|v| {
                        v.binding
                            .as_ref()
                            .is_some_and(|b| b.group == 0 && b.binding == 0)
                    })
                    .unwrap();
                let wgpu::naga::TypeInner::Struct { members, span } =
                    &module.types[view_global.ty].inner
                else {
                    panic!("expected view uniform struct")
                };
                let mut view_bytes = vec![0u8; *span as usize];
                let offset = members
                    .iter()
                    .find(|m| m.name.as_deref() == Some("viewport"))
                    .unwrap()
                    .offset as usize;
                view_bytes[offset..offset + 16]
                    .copy_from_slice(bytemuck::cast_slice(&[0.0f32, 0.0, 100.0, 100.0]));
                for (name, matrix) in [
                    ("view_from_world", Mat4::IDENTITY),
                    (
                        "clip_from_view",
                        Mat4::perspective_infinite_reverse_rh(
                            std::f32::consts::FRAC_PI_2,
                            1.0,
                            0.1,
                        ),
                    ),
                ] {
                    let offset = members
                        .iter()
                        .find(|m| m.name.as_deref() == Some(name))
                        .unwrap()
                        .offset as usize;
                    view_bytes[offset..offset + 64]
                        .copy_from_slice(bytemuck::cast_slice(&matrix.to_cols_array()));
                }
                use wgpu::util::DeviceExt;
                let view = raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: &view_bytes,
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("production Gaussian raster probe"),
                    source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
                });
                let view_layout = raw.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
                let output_layout =
                    raw.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries: &[wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        }],
                    });
                let layout = raw.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&view_layout), Some(&output_layout)],
                    immediate_size: 0,
                });
                let pipeline = raw.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: None,
                    layout: Some(&layout),
                    module: &shader,
                    entry_point: Some("raster_probe"),
                    compilation_options: default(),
                    cache: None,
                });
                let output = raw.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 336,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let staging = raw.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 336,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let group = raw.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &output_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 1,
                        resource: output.as_entire_binding(),
                    }],
                });
                let view_group = raw.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &view_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: view.as_entire_binding(),
                    }],
                });
                let mut encoder = device.create_command_encoder(&default());
                {
                    let mut pass = encoder.begin_compute_pass(&default());
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &view_group, &[]);
                    pass.set_bind_group(1, &group, &[]);
                    pass.dispatch_workgroups(6, 1, 1);
                }
                encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, 336);
                queue.submit([encoder.finish()]);
                let (send, receive) = std::sync::mpsc::channel();
                staging
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |r| send.send(r).unwrap());
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                receive.recv().unwrap().unwrap();
                let rows =
                    bytemuck::cast_slice::<u8, [f32; 4]>(&staging.slice(..).get_mapped_range())
                        .to_vec();
                let values = rows[..12].chunks_exact(2).map(|p| p[0]).collect::<Vec<_>>();
                staging.unmap();
                eprintln!("profile={raster_profile:?} adaptive={adaptive} aabb={aabb}: {rows:?}");
                let near = |actual: f32, expected: f32| {
                    assert!(
                        (actual - expected).abs() < 0.00001,
                        "actual={actual}, expected={expected}"
                    )
                };
                near(values[0][0], 0.1);
                if raster_profile == GaussianRasterProfile::NormalizedGaussian {
                    for (actual, expected) in rows[19][..3].iter().zip([2.3, 1.0, 2.3]) {
                        near(*actual, expected);
                    }
                    for (actual, expected) in rows[20][..3].iter().zip([2.34, 1.02, 2.31]) {
                        near(*actual, expected);
                    }
                    if !aabb {
                        assert!(rows[19][3] > 0.0, "world ellipse must retain its NDC slope");
                    }
                    if !aabb {
                        for (row, expected_power) in
                            rows[13..].iter().zip([-1.0, -3.0, -0.5, -0.5, -0.5, -1.0])
                        {
                            assert!(
                                row.iter().all(|v| v.is_finite()),
                                "non-finite physical-point result: {row:?}"
                            );
                            near(row[0], expected_power);
                            near(row[1], expected_power);
                            near(row[2], row[3]);
                        }
                    }
                    assert_eq!(rows[12], [1.0, 0.0, 0.0, 1.0]);
                    near(values[1][0], 0.035608574);
                    near(values[2][0], 0.0);
                    near(values[3][0], 0.0);
                    near(values[4][0], 0.35608575);
                    near(values[5][0], 1.0);
                    for row in &values {
                        near(row[1], 2.8284271);
                        near(row[3], -1.0);
                    }
                    let expected = if aabb {
                        [0.048989795, 0.048989795, 4.8989795, 4.8989795]
                    } else {
                        [0.054641016, 0.014641016, 5.4641016, 1.4641016]
                    };
                    for pair in rows[..12].chunks_exact(2) {
                        for (actual, expected) in pair[1].into_iter().zip(expected) {
                            near(actual, expected);
                        }
                    }
                } else {
                    for (actual, expected) in rows[19][..3].iter().zip([2.3, -1.0, 2.3]) {
                        near(*actual, expected);
                    }
                    for (actual, expected) in rows[20][..3].iter().zip([2.34, -1.02, 2.31]) {
                        near(*actual, expected);
                    }
                    assert_eq!(rows[12], [0.0; 4]);
                    near(values[1][0], 0.032465246);
                    near(values[2][0], 0.0011108996);
                    near(values[3][0], 0.0003606563);
                    near(values[5][0], 0.999);
                    near(values[0][1], if adaptive { 2.0963855 } else { 3.0 });
                    for row in &values {
                        near(row[3], -3.0);
                    }
                    if !adaptive {
                        let expected = if aabb {
                            [0.051961523, 0.051961523, 5.196152, 5.196152]
                        } else {
                            [-0.015529141, 0.057955544, -1.5529141, 5.7955546]
                        };
                        for pair in rows[..12].chunks_exact(2) {
                            for (actual, expected) in pair[1].into_iter().zip(expected) {
                                near(actual, expected);
                            }
                        }
                    }
                }
            }
        }
    }
}
