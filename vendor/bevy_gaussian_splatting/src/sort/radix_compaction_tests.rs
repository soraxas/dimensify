//! Compose actual production imports and radix shader, never a shader copy.
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
                let handle = assets.add(Shader::from_wgsl(
                    source,
                    path.to_string_lossy().into_owned(),
                ));
                handles.push(handle);
            }
        }
    }
}

fn production_module(bits: RadixSortDepthBits) -> wgpu::naga::Module {
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
        for (index, name) in [
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
                found[index] = true;
            }
        }
    }
    assert!(
        found.into_iter().all(|present| present),
        "shader tests need resolved Bevy 0.19.1 render/pbr/core_pipeline registry sources under CARGO_HOME; run cargo fetch first"
    );
    assets
        .insert(
            RADIX_SHADER_HANDLE.id(),
            Shader::from_wgsl(include_str!("radix.wgsl"), "radix.wgsl"),
        )
        .unwrap();
    let mut cache = ShaderCache::new(
        (),
        wgpu::Features::empty(),
        wgpu::DownlevelFlags::all(),
        |_, source, _| {
            let ShaderCacheSource::Naga(module) = source else {
                panic!("expected Naga source")
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
    let defs = shader_defs_with_defines(
        CloudPipelineKey::default(),
        ShaderDefines::for_radix_depth_bits(bits),
    );
    (*cache.get(0, RADIX_SHADER_HANDLE.id(), &defs).unwrap()).clone()
}

#[test]
fn production_radix_workgroups_fit_256_invocation_devices() {
    for bits in [
        RadixSortDepthBits::Bits16,
        RadixSortDepthBits::Bits24,
        RadixSortDepthBits::Bits32,
    ] {
        let module = production_module(bits);
        for entry in &module.entry_points {
            let invocations: u32 = entry.workgroup_size.iter().product();
            assert!(
                invocations <= 256,
                "{} ({bits:?}) requires {invocations} invocations, exceeding the WebGPU baseline",
                entry.name
            );
        }
    }
}

#[test]
fn production_compaction_shaders_validate_all_precisions() {
    for bits in [
        RadixSortDepthBits::Bits16,
        RadixSortDepthBits::Bits24,
        RadixSortDepthBits::Bits32,
    ] {
        let module = production_module(bits);
        use bevy::render::render_resource::ShaderType;
        assert_eq!(
            uniform_bytes(&module, 0, 1, &[]).len() as u64,
            CloudUniform::min_size().get()
        );
        for entry in [
            "compact_count",
            "compact_scan",
            "compact_scatter",
            "radix_sort_c_count_tiles",
            "radix_sort_c_scan_tiles",
            "radix_sort_c_scatter",
        ] {
            assert!(
                module.entry_points.iter().any(|p| p.name == entry),
                "missing {entry}"
            );
        }
    }
}

#[test]
fn visibility_compaction_is_explicit_and_defaults_off() {
    let mut json = serde_json::to_value(CloudSettings::default()).unwrap();
    assert_eq!(
        json.get("visibility_compaction"),
        Some(&serde_json::json!(false))
    );
    json.as_object_mut()
        .unwrap()
        .remove("visibility_compaction");
    assert!(
        !serde_json::from_value::<CloudSettings>(json)
            .unwrap()
            .visibility_compaction
    );
}

fn uniform_bytes(
    module: &wgpu::naga::Module,
    binding: u32,
    group: u32,
    values: &[(&str, &[u8])],
) -> Vec<u8> {
    let global = module
        .global_variables
        .iter()
        .map(|(_, v)| v)
        .find(|v| {
            v.binding
                .as_ref()
                .is_some_and(|b| b.group == group && b.binding == binding)
        })
        .unwrap();
    let wgpu::naga::TypeInner::Struct { members, span } = &module.types[global.ty].inner else {
        panic!("expected struct")
    };
    let mut bytes = vec![0; *span as usize];
    for (name, value) in values {
        let member = members
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .unwrap();
        bytes[member.offset as usize..member.offset as usize + value.len()].copy_from_slice(value);
    }
    bytes
}

fn readback(
    device: &RenderDevice,
    queue: &bevy::render::renderer::RenderQueue,
    buffer: &wgpu::Buffer,
) -> Vec<u32> {
    let staging = device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: buffer.size(),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&default());
    encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, buffer.size());
    queue.submit([encoder.finish()]);
    let (send, recv) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| send.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    recv.recv().unwrap().unwrap();
    let result = bytemuck::cast_slice::<u8, u32>(&staging.slice(..).get_mapped_range()).to_vec();
    staging.unmap();
    result
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn production_compaction_gpu_readback() {
    production_radix_gpu_readback(false, false);
}

#[test]
#[ignore = "requires native GPU; requests a device limited to 256 workgroup invocations"]
fn production_radix_gpu_readback_with_256_invocation_limit() {
    production_radix_gpu_readback(true, false);
}

#[test]
#[ignore = "offscreen 4M-entry latency acceptance; set DIMENSIFY_RADIX_LATENCY_BUDGET_MS for the test GPU"]
fn production_radix_large_cloud_latency() {
    production_radix_gpu_readback(true, true);
}

#[test]
#[ignore = "headless native GPU; validates full production layouts against WebGPU baseline limits"]
fn production_radix_layout_fits_eight_storage_buffers() {
    use crate::gaussian::formats::planar_3d::Gaussian3d;
    let settings = bevy::render::settings::WgpuSettings {
        constrained_limits: Some(wgpu::Limits {
            max_storage_buffers_per_shader_stage: 8,
            max_compute_invocations_per_workgroup: 256,
            max_compute_workgroup_storage_size: 32768,
            ..default()
        }),
        ..default()
    };
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &settings,
    ));
    eprintln!("production layout adapter={:?}", resources.2.0);
    let device = resources.0;
    assert_eq!(device.limits().max_storage_buffers_per_shader_stage, 8);
    let mut world = World::new();
    world.insert_resource(device.clone());
    world.init_resource::<CloudPipeline<Gaussian3d>>();
    let radix = RadixSortPipeline::<Gaussian3d>::from_world(&mut world);
    // Do not rebuild a simplified layout in this test: the full production
    // layout includes bindings that are unused by the composed entry point.
    let layouts = radix
        .sorting_layout
        .iter()
        .map(|descriptor| {
            device.create_bind_group_layout(Some(descriptor.label.as_ref()), &descriptor.entries)
        })
        .collect::<Vec<_>>();
    let raw = device.wgpu_device();
    let layout = raw.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("full production radix layout / baseline WebGPU limits"),
        bind_group_layouts: &layouts
            .iter()
            .map(|layout| Some(&**layout))
            .collect::<Vec<_>>(),
        immediate_size: 0,
    });
    for bits in [
        RadixSortDepthBits::Bits16,
        RadixSortDepthBits::Bits24,
        RadixSortDepthBits::Bits32,
    ] {
        let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("production radix"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(production_module(bits))),
        });
        for entry in RADIX_ENTRYPOINTS {
            let _pipeline = raw.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: default(),
                cache: None,
            });
        }
    }
}

fn production_radix_gpu_readback(limit_invocations: bool, benchmark: bool) {
    use crate::gaussian::formats::planar_3d::Gaussian3d;
    use wgpu::util::DeviceExt;
    let settings = bevy::render::settings::WgpuSettings {
        #[cfg(feature = "gpu_timing")]
        disabled_features: Some(
            wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
                | wgpu::Features::PIPELINE_STATISTICS_QUERY,
        ),
        constrained_limits: limit_invocations.then_some(wgpu::Limits {
            max_compute_invocations_per_workgroup: 256,
            // This test isolates invocation portability, not the separate
            // workgroup-storage requirement of the existing scatter stage.
            max_compute_workgroup_storage_size: 32768,
            ..default()
        }),
        ..default()
    };
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &settings,
    ));
    eprintln!("radix correctness adapter={:?}", resources.2.0);
    let device = resources.0;
    let queue = resources.1;
    #[cfg(feature = "gpu_timing")]
    let mut timing =
        crate::gpu_timing::tests::Probe::new(device.clone(), queue.clone(), resources.2);
    if limit_invocations {
        assert_eq!(device.limits().max_compute_invocations_per_workgroup, 256);
    }
    let raw = device.wgpu_device();
    let mut world = World::new();
    world.insert_resource(device.clone());
    world.init_resource::<CloudPipeline<Gaussian3d>>();
    let radix = RadixSortPipeline::<Gaussian3d>::from_world(&mut world);
    for bits in [
        RadixSortDepthBits::Bits16,
        RadixSortDepthBits::Bits24,
        RadixSortDepthBits::Bits32,
    ] {
        if benchmark && bits != RadixSortDepthBits::Bits32 {
            continue;
        }
        let defs = ShaderDefines::for_radix_depth_bits(bits);
        let module = production_module(bits);
        let make_layout = |types: &[wgpu::BufferBindingType]| {
            raw.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &types
                    .iter()
                    .enumerate()
                    .map(|(i, ty)| wgpu::BindGroupLayoutEntry {
                        binding: i as u32,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: *ty,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect::<Vec<_>>(),
            })
        };
        let uniform = wgpu::BufferBindingType::Uniform;
        let storage = wgpu::BufferBindingType::Storage { read_only: false };
        let layouts = [
            make_layout(&[uniform]),
            make_layout(&[uniform]),
            radix.radix_cloud_layout.value().clone(),
            make_layout(&[uniform, storage, storage, storage, storage, storage]),
        ];
        let pipeline_layout = raw.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &layouts.iter().map(Some).collect::<Vec<_>>(),
            immediate_size: 0,
        });
        let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("production radix"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module.clone())),
        });
        let entries = [
            "radix_reset",
            "radix_sort_a",
            "radix_sort_b",
            "radix_sort_c_count_tiles",
            "radix_sort_c_scan_tiles",
            "radix_sort_c_scatter",
            "compact_count",
            "compact_scan",
            "compact_scatter",
        ];
        let pipelines = entries.map(|entry| {
            raw.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: default(),
                cache: None,
            })
        });
        let counts: &[usize] = if benchmark {
            &[4_000_000]
        } else {
            &[
                0, 1, 31, 257, 512, 1023, 1024, 1025, 4095, 4096, 4097, 4099, 8193,
            ]
        };
        for &count in counts {
            let wide = benchmark && std::env::var_os("DIMENSIFY_RADIX_BENCH_WIDE").is_some();
            // Reuse buffers through all -> mixed -> none -> all camera views, to expose stale tails.
            let positions = (0..count.max(1))
                .map(|i| {
                    [
                        if wide {
                            0.
                        } else if count == 512 || count == 1025 || i % 3 == 0 {
                            0.25
                        } else {
                            -0.25
                        },
                        0.,
                        // An all-tied multi-round tile plus many nonuniform
                        // digits crossing tiles, alongside the original ties.
                        // Binary fractions keep the CPU/GPU depth oracle exact.
                        if wide {
                            let bits = (i as u32)
                                .wrapping_mul(747_796_405)
                                .wrapping_add(2_891_336_453);
                            -f32::from_bits(0x3f80_0000 + bits % 0x0300_0000)
                        } else if count == 512 || count == 1025 {
                            -2.0
                        } else if count == 8193 {
                            -2.0 - (i.wrapping_mul(73) % 1024) as f32 / 128.
                        } else {
                            -2.0 - (i % 7) as f32 * 0.125
                        },
                        1.,
                    ]
                })
                .collect::<Vec<_>>();
            let make = |bytes: &[u8], usage| {
                raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytes,
                    usage,
                })
            };
            let pos = make(
                bytemuck::cast_slice(&positions),
                wgpu::BufferUsages::STORAGE,
            );
            let scratch = GpuRadixBuffers::new(count.max(1), &device);
            let sorted = make(
                &vec![0; count.max(1) * 8],
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            );
            let draw = make(
                &[0; 16],
                wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            );
            let bind = |index: usize, buffers: &[&wgpu::Buffer]| {
                raw.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &layouts[index],
                    entries: &buffers
                        .iter()
                        .enumerate()
                        .map(|(i, b)| wgpu::BindGroupEntry {
                            binding: i as u32,
                            resource: b.as_entire_binding(),
                        })
                        .collect::<Vec<_>>(),
                })
            };
            // Use the production cloud layout, including its unused planes;
            // the old position-only harness missed the nine-buffer regression.
            let unused_planes = Gaussian3d::min_binding_sizes()
                .iter()
                .skip(1)
                .map(|size| make(&vec![0u8; *size], wgpu::BufferUsages::STORAGE))
                .collect::<Vec<_>>();
            let cloud_buffers = std::iter::once(&pos)
                .chain(unused_planes.iter())
                .collect::<Vec<_>>();
            let cloud_group = bind(2, &cloud_buffers);
            let sort_groups = (0..defs.radix_digit_places as usize)
                .flat_map(|pass| {
                    (0..2)
                        .map(|parity| {
                            let (input, output): (&wgpu::Buffer, &wgpu::Buffer) = if parity == 0 {
                                (&sorted, &scratch.entry_buffer_b)
                            } else {
                                (&scratch.entry_buffer_b, &sorted)
                            };
                            bind(
                                3,
                                &[
                                    &scratch.sorting_pass_buffers[pass],
                                    &scratch.sorting_global_buffer,
                                    &scratch.sorting_status_counter_buffer,
                                    &draw,
                                    input,
                                    output,
                                ],
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            // Position-only sorting must conservatively retain off-screen centres:
            // their unknown Gaussian footprint can still overlap the viewport.
            // Exercise all -> off-screen -> mixed -> none -> all on the same buffers.
            for (camera_x, camera_z) in
                [(0.0f32, 0.0f32), (4., 0.), (0., -2.5), (0., -4.), (0., 0.)]
            {
                let view_from_world = Mat4::from_translation(Vec3::new(-camera_x, 0., -camera_z));
                let projection =
                    Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1);
                let matrix = (projection * view_from_world).to_cols_array();
                let view_data = uniform_bytes(
                    &module,
                    0,
                    0,
                    &[
                        ("unjittered_clip_from_world", bytemuck::cast_slice(&matrix)),
                        (
                            "clip_from_view",
                            bytemuck::cast_slice(&projection.to_cols_array()),
                        ),
                        (
                            "view_from_world",
                            bytemuck::cast_slice(&view_from_world.to_cols_array()),
                        ),
                        (
                            "world_position",
                            bytemuck::cast_slice(&[camera_x, 0., camera_z]),
                        ),
                    ],
                );
                let view = make(&view_data, wgpu::BufferUsages::UNIFORM);
                let view_group = bind(0, &[&view]);
                let mut reference = positions
                    .iter()
                    .take(count)
                    .enumerate()
                    .filter(|(_, p)| p[2] < camera_z)
                    .map(|(i, p)| {
                        let dist2 = (p[0] - camera_x).powi(2) + (p[2] - camera_z).powi(2);
                        (
                            (u32::MAX - dist2.to_bits()) >> defs.radix_key_shift,
                            i as u32,
                        )
                    })
                    .collect::<Vec<_>>();
                reference.sort_by_key(|v| v.0);
                let mut all_reference = positions
                    .iter()
                    .take(count)
                    .enumerate()
                    .map(|(i, p)| {
                        let visible = p[2] < camera_z;
                        let dist2 = (p[0] - camera_x).powi(2) + (p[2] - camera_z).powi(2);
                        let key = if visible {
                            u32::MAX - dist2.to_bits()
                        } else {
                            u32::MAX
                        };
                        (key >> defs.radix_key_shift, i as u32)
                    })
                    .collect::<Vec<_>>();
                all_reference.sort_by_key(|v| v.0);
                let mut baseline_visible = Vec::new();
                for compact in [false, true] {
                    let cloud_data = uniform_bytes(
                        &module,
                        0,
                        1,
                        &[
                            (
                                "transform",
                                bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
                            ),
                            ("count", bytemuck::bytes_of(&(count as u32))),
                            (
                                "visibility_compaction",
                                bytemuck::bytes_of(&u32::from(compact)),
                            ),
                        ],
                    );
                    let cloud_uniform = make(&cloud_data, wgpu::BufferUsages::UNIFORM);
                    let uniform_group = bind(1, &[&cloud_uniform]);
                    let mut latencies = Vec::new();
                    for sample in 0..if benchmark { 12 } else { 1 } {
                        let begin = std::time::Instant::now();
                        let mut encoder = device.create_command_encoder(&default());
                        #[cfg(feature = "gpu_timing")]
                        let interval = timing.record(count, compact);
                        encode_radix_sort(
                            &mut encoder,
                            &pipelines.each_ref(),
                            defs,
                            count as u32,
                            compact,
                            &scratch,
                            &draw,
                            [
                                (&view_group, &[]),
                                (&uniform_group, &[]),
                                (&cloud_group, &[]),
                            ],
                            &sort_groups.iter().collect::<Vec<_>>(),
                            #[cfg(all(
                                feature = "render_diagnostics",
                                not(feature = "gpu_timing")
                            ))]
                            None,
                            #[cfg(feature = "gpu_timing")]
                            Some(&interval),
                        );
                        queue.submit([encoder.finish()]);
                        #[cfg(feature = "gpu_timing")]
                        timing.finish(count, compact);
                        if benchmark {
                            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                            if sample >= 2 {
                                latencies.push(begin.elapsed().as_secs_f64() * 1000.);
                            }
                        }
                    }
                    let draw_args = readback(&device, &queue, &draw);
                    assert_eq!(draw_args[0], 4);
                    assert_eq!(
                        draw_args[1] as usize,
                        if compact { reference.len() } else { count }
                    );
                    let output = readback(&device, &queue, &sorted);
                    let pairs = output
                        .chunks_exact(2)
                        .take(draw_args[1] as usize)
                        .map(|p| (p[0], p[1]))
                        .collect::<Vec<_>>();
                    if compact {
                        assert_eq!(
                            pairs, reference,
                            "count={count}, bits={bits:?}, camera={camera_x}"
                        );
                        assert_eq!(pairs, baseline_visible);
                    } else {
                        assert_eq!(
                            pairs, all_reference,
                            "baseline must preserve every entry and equal-key ordering: count={count}, bits={bits:?}, camera={camera_x}"
                        );
                        baseline_visible = pairs
                            .into_iter()
                            .filter(|(_, i)| positions[*i as usize][2] < camera_z)
                            .collect();
                    }
                    if benchmark {
                        latencies.sort_by(f64::total_cmp);
                        let median = latencies[latencies.len() / 2];
                        eprintln!(
                            "4M full-precision radix wide={wide} encode + submit + completion latency (not GPU timestamps): {latencies:?} ms"
                        );
                        if let Ok(budget) = std::env::var("DIMENSIFY_RADIX_LATENCY_BUDGET_MS") {
                            let budget: f64 = budget.parse().expect("numeric latency budget");
                            assert!(
                                median <= budget,
                                "median {median:.3} ms exceeds explicit {budget:.3} ms test-GPU budget"
                            );
                        }
                        return;
                    }
                }
            }
        }
    }
}

#[test]
fn readiness_requires_only_baseline_stages_until_compaction_is_enabled() {
    let mut variant = RadixSortPipelineVariant {
        shader_defines: ShaderDefines::default(),
        radix_sort_pipelines: [None; RADIX_PIPELINE_COUNT],
    };
    // Required IDs must exist; an absent opt-in stage must never pass readiness.
    assert_eq!(variant.required_pipelines(false).len(), 6);
    assert_eq!(variant.required_pipelines(true).len(), 9);
    for pipeline in &mut variant.radix_sort_pipelines[..6] {
        *pipeline = Some(CachedComputePipelineId::INVALID);
    }
    assert!(
        variant
            .required_pipelines(false)
            .iter()
            .all(Option::is_some)
    );
    assert!(variant.required_pipelines(true).iter().any(Option::is_none));
}
