use super::tests::{fixture, webp};
use crate::io::sog_packed::{DecodeLimits, decode};
use crate::render::sog::work::{BakeRange, CompactBaker, CompactWork, LocalView, WORK_SHADER};
use crate::render::sog::{DECODE_SHADER, GpuSogSource};

const PROBE: &str = r#"
@group(1) @binding(0) var<storage, read_write> output: array<vec4f>;
@compute @workgroup_size(1)
fn probe(@builtin(global_invocation_id) id: vec3u) {
    let i = id.x;
    if i >= sog_info.count { return; }
    let v = sog_decode(i);
    let base = i * 22u;
    output[base] = vec4f(v.position, 1.0);
    output[base + 1u] = v.rotation;
    output[base + 2u] = vec4f(v.scale, v.opacity);
    output[base + 3u] = vec4f(sog_color(i, vec3f(1.0, 0.0, 0.0)), 1.0);
    output[base + 4u] = vec4f(sog_color(i, vec3f(0.0, 1.0, 0.0)), 1.0);
    output[base + 5u] = vec4f(sog_color(i, normalize(vec3f(-1.0, 2.0, -3.0))), 1.0);
    for (var c = 0u; c < 16u; c++) {
        output[base + 6u + c] = vec4f(sog_coefficient(i, c), 1.0);
    }
}
"#;

fn module() -> wgpu::naga::Module {
    let source = format!("{DECODE_SHADER}\n{PROBE}");
    let module = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("portable shader validation");
    module
}

#[test]
fn packed_gpu_shader_is_portable_without_f16_or_extra_features() {
    module();
}

#[test]
fn compact_work_shader_and_range_contract_are_portable() {
    let source = format!("{DECODE_SHADER}\n{WORK_SHADER}");
    let parsed = wgpu::naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&parsed)
    .expect("portable compact kernel");
    assert_eq!(CompactWork::required_bytes(100), 2400);
    assert!(
        BakeRange {
            source_start: 2,
            count: 2,
            destination_start: 0
        }
        .validate(3, 5)
        .is_err()
    );
    assert!(
        BakeRange {
            source_start: 0,
            count: 2,
            destination_start: 4
        }
        .validate(3, 5)
        .is_err()
    );
    assert!(
        BakeRange {
            source_start: u32::MAX,
            count: 1,
            destination_start: 0
        }
        .validate(u32::MAX, 5)
        .is_err()
    );
    assert!(
        BakeRange {
            source_start: 0,
            count: 3,
            destination_start: 2
        }
        .validate(3, 5)
        .is_ok()
    );
    assert!(LocalView::perspective([f32::NAN, 0.0, 0.0]).is_err());
    assert!(LocalView::orthographic([0.0; 3]).is_err());
}

// Independent real-SH polynomial oracle; coefficient values come from the CPU
// decoder, not a second copy of the GPU texture addressing/decoder.
fn basis(d: [f32; 3]) -> [f32; 16] {
    let [x, y, z] = d;
    [
        0.2820948,
        -0.48860252 * y,
        0.48860252 * z,
        -0.48860252 * x,
        1.0925485 * x * y,
        -1.0925485 * y * z,
        0.31539157 * (2.0 * z * z - x * x - y * y),
        -1.0925485 * x * z,
        0.54627424 * (x * x - y * y),
        -0.5900436 * y * (3.0 * x * x - y * y),
        2.8906114 * x * y * z,
        -0.4570458 * y * (4.0 * z * z - x * x - y * y),
        0.37317634 * z * (2.0 * z * z - 3.0 * x * x - 3.0 * y * y),
        -0.4570458 * x * (4.0 * z * z - x * x - y * y),
        1.4453057 * z * (x * x - y * y),
        -0.5900436 * x * (x * x - 3.0 * y * y),
    ]
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture and sh3"]
fn packed_gpu_matches_cpu_attributes_and_all_sh_bands() {
    assert_eq!(
        crate::material::spherical_harmonics::SH_DEGREE,
        3,
        "run this oracle with --features headless,io_sog,sh3"
    );
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    eprintln!("packed SOG probe adapter={:?}", resources.2.0);
    let device = resources.0;
    let queue = resources.1;
    let raw = device.wgpu_device();
    let source_layout = GpuSogSource::layout(raw);
    let baker = CompactBaker::new(raw).expect("compact pipeline");
    let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quantized SOG decoder oracle"),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module())),
    });
    let output_layout = raw.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let pipeline_layout = raw.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&source_layout), Some(&output_layout)],
        immediate_size: 0,
    });
    let pipeline = raw.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("probe"),
        compilation_options: Default::default(),
        cache: None,
    });
    for encoded_bands in 0..=3 {
        let (mut meta, mut files) = fixture(3, encoded_bands > 0);
        if encoded_bands == 0 {
            meta.means.mins[0] = 0.0;
            meta.means.maxs[0] = 0.0;
        }
        files.insert(
            "quats.webp".into(),
            webp(
                &[
                    [160, 100, 140, 252 + encoded_bands as u8],
                    [100, 150, 110, 253],
                    [110, 160, 100, 254],
                    [128, 128, 128, 255],
                ],
                2,
                2,
            ),
        );
        if let Some(sh) = meta.sh_n.as_mut() {
            sh.bands = encoded_bands;
            sh.count = 257;
            sh.codebook = (0..256).map(|i| (i as f32 - 128.0) / 512.0).collect();
            let coefficients = [0, 3, 8, 15][encoded_bands];
            let width = coefficients * 64;
            let pixels = (0..width * 5)
                .map(|i| {
                    [
                        ((i * 7) % 256) as u8,
                        ((i * 13 + 1) % 256) as u8,
                        ((i * 17 + 3) % 256) as u8,
                        255,
                    ]
                })
                .collect::<Vec<_>>();
            files.insert("shN_centroids.webp".into(), webp(&pixels, width as u32, 5));
            files.insert(
                "shN_labels.webp".into(),
                webp(
                    &[[0, 0, 0, 0], [64, 0, 0, 0], [0, 1, 0, 0], [255, 255, 0, 0]],
                    2,
                    2,
                ),
            ); // padding label deliberately invalid: it is not a splat
        }
        let cpu = super::decode_sog(&meta, &files).expect("CPU oracle");
        for requested_bands in 0..=3 {
            let packed = decode(
                &meta,
                &files,
                requested_bands,
                DecodeLimits {
                    max_texture_dimension: 4096,
                    max_texture_bytes: 1 << 20,
                },
            )
            .expect("packed source");
            let mut shared_palette = None;
            for logical_range in [None, Some(0..3), Some(1..3), Some(2..3)] {
                let first = logical_range
                    .as_ref()
                    .map_or(0, |range| range.start as usize);
                let page = logical_range.map(|range| {
                    packed
                        .copy_range(
                            range,
                            DecodeLimits {
                                max_texture_dimension: 4096,
                                max_texture_bytes: 1 << 20,
                            },
                        )
                        .expect("independent range page")
                });
                let packed = page.as_ref().unwrap_or(&packed);
                let expected_bytes = GpuSogSource::required_bytes(packed);
                assert!(
                    GpuSogSource::upload(raw, &queue, &source_layout, &packed, expected_bytes - 1)
                        .is_err()
                );
                // Exercise partial rows/planes, zero allowance and publication only
                // after all texels have arrived. The same compute oracle verifies
                // every SH band and the final padding row below.
                let mut upload = if let Some(palette) = shared_palette.as_ref() {
                    if first == 1 {
                        let mut other_meta = meta.clone();
                        other_meta.scales.codebook[0] -= 0.125;
                        let other = decode(
                            &other_meta,
                            &files,
                            requested_bands,
                            DecodeLimits {
                                max_texture_dimension: 4096,
                                max_texture_bytes: 1 << 20,
                            },
                        )
                        .unwrap();
                        assert!(matches!(
                            GpuSogSource::begin_upload_with_palette(
                                raw,
                                &queue,
                                &source_layout,
                                &other,
                                palette,
                                u64::MAX,
                            ),
                            Err(crate::render::sog::UploadError::Palette)
                        ));
                    }
                    let budget = expected_bytes
                        - packed.palette_texture_bytes()
                        - 4096
                        - if packed.higher_order().is_none() {
                            4
                        } else {
                            0
                        };
                    assert!(
                        GpuSogSource::begin_upload_with_palette(
                            raw,
                            &queue,
                            &source_layout,
                            packed,
                            palette,
                            budget - 1,
                        )
                        .is_err()
                    );
                    GpuSogSource::begin_upload_with_palette(
                        raw,
                        &queue,
                        &source_layout,
                        packed,
                        palette,
                        budget,
                    )
                } else {
                    GpuSogSource::begin_upload(raw, &queue, &source_layout, packed, expected_bytes)
                }
                .expect("reserve upload");
                assert_eq!(
                    upload.resident_bytes(),
                    expected_bytes
                        - shared_palette
                            .as_ref()
                            .map_or(0, |palette| palette.resident_bytes())
                );
                assert!(!upload.is_complete());
                assert_eq!(upload.write(&queue, &packed, 0), 0);
                let mut written = 0;
                while !upload.is_complete() {
                    let bytes = upload.write(&queue, &packed, 4096);
                    assert!(bytes > 0 && bytes <= 4096);
                    written += bytes;
                }
                assert_eq!(
                    written as u64,
                    packed.texture_bytes()
                        - if shared_palette.is_some() {
                            packed.palette_texture_bytes()
                        } else {
                            0
                        }
                        + if packed.higher_order().is_none() {
                            if shared_palette.is_some() { 4 } else { 8 }
                        } else {
                            0
                        }
                );
                let source = upload.finish().ok().expect("complete immutable source");
                assert_eq!(source.resident_bytes(), expected_bytes);
                assert_eq!(
                    source.exclusive_bytes() + source.palette().resident_bytes(),
                    expected_bytes
                );
                if let Some(palette) = shared_palette.as_ref() {
                    assert!(std::sync::Arc::ptr_eq(source.palette(), palette));
                } else {
                    shared_palette = Some(source.palette().clone());
                }
                let size = 4 * 22 * 16; // one extra row must remain unwritten
                let output = raw.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                });
                let staging = raw.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = raw.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &output_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: output.as_entire_binding(),
                    }],
                });
                let mut encoder = raw.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, source.bind_group(), &[]);
                    pass.set_bind_group(1, &group, &[]);
                    pass.dispatch_workgroups(4, 1, 1);
                }
                encoder.copy_buffer_to_buffer(&output, 0, &staging, 0, size);
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
                staging.unmap();
                let near = |actual: f32, expected: f32| {
                    assert!(
                        (actual - expected).abs() < 0.00003,
                        "encoded={encoded_bands} requested={requested_bands}: actual={actual}, expected={expected}"
                    );
                };
                let bands = requested_bands.min(encoded_bands as u32);
                let count = [1, 4, 9, 16][bands as usize];
                for local in 0..packed.count() as usize {
                    let i = first + local;
                    let row = &rows[local * 22..(local + 1) * 22];
                    for axis in 0..3 {
                        near(row[0][axis], cpu.position_visibility[i].position[axis]);
                        near(row[2][axis], cpu.scale_opacity[i].scale[axis]);
                    }
                    near(row[2][3], cpu.scale_opacity[i].opacity);
                    // Legacy CPU storage is wxyz; the GPU decode contract is xyzw.
                    let q = cpu.rotation[i].rotation;
                    let expected = [q[1], q[2], q[3], q[0]];
                    let dot: f32 = row[1].iter().zip(expected).map(|(a, b)| a * b).sum();
                    near(dot.abs(), 1.0);
                    let coefficients = &cpu.spherical_harmonic[i].coefficients;
                    for c in 0..16 {
                        for channel in 0..3 {
                            near(
                                row[6 + c][channel],
                                if c < count {
                                    coefficients[c * 3 + channel]
                                } else {
                                    0.0
                                },
                            );
                        }
                    }
                    for (view, direction) in [
                        [1., 0., 0.],
                        [0., 1., 0.],
                        bevy::math::Vec3::new(-1., 2., -3.).normalize().to_array(),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let b = basis(direction);
                        for channel in 0..3 {
                            let color = 0.5
                                + (0..count)
                                    .map(|c| b[c] * coefficients[c * 3 + channel])
                                    .sum::<f32>();
                            near(row[3 + view][channel], color.max(0.0));
                        }
                    }
                }
                assert!(
                    rows[packed.count() as usize * 22..]
                        .iter()
                        .all(|v| *v == [0.0; 4]),
                    "padded tail was decoded"
                );
                if packed.count() == 3 {
                    compact_ranges_and_color_refresh(raw, &queue, &source, &baker, &rows);
                }
            }
            let weak = std::sync::Arc::downgrade(shared_palette.as_ref().unwrap());
            drop(shared_palette);
            assert!(
                weak.upgrade().is_none(),
                "last page/cache owner must release palette"
            );
        }
    }
}

fn read_work(device: &wgpu::Device, queue: &wgpu::Queue, work: &CompactWork) -> [Vec<u32>; 3] {
    let buffers = work.buffers();
    let mut encoder = device.create_command_encoder(&Default::default());
    let staging = buffers.map(|buffer| {
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: buffer.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(buffer, 0, &read, 0, buffer.size());
        read
    });
    queue.submit([encoder.finish()]);
    staging.map(|read| {
        let (send, receive) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |r| send.send(r).unwrap());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();
        let values = bytemuck::cast_slice::<u8, u32>(&read.slice(..).get_mapped_range()).to_vec();
        read.unmap();
        values
    })
}

/// Characterization probe for the real cache/encoder. The counts describe the
/// current policy; readback and encoder-use assertions guard exact reuse.
#[test]
#[ignore = "requires native GPU; offscreen movement-work characterization"]
fn packed_bake_work_tracks_motion_and_partial_reselection() {
    use crate::render::{
        compact::CompactCloud,
        sog::{
            diagnostics::BakeDiagnostics,
            view::{BakeCache, BakeState, SourceRange},
        },
    };
    use bevy::{prelude::Entity, render::renderer::WgpuWrapper};
    use std::sync::Arc;

    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    eprintln!("bake-work adapter={:?}", resources.2.0);
    let device = resources.0.wgpu_device();
    let queue = &resources.1;
    let layout = GpuSogSource::layout(device);
    let baker = Arc::new(WgpuWrapper::new(CompactBaker::new(device).unwrap()));
    let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
    let pair_bytes = 48_u64.div_ceil(alignment) * alignment + 48;
    for bands in [0, 3] {
        let (mut meta, mut files) = fixture(3, bands > 0);
        if let Some(sh) = meta.sh_n.as_mut() {
            sh.bands = 3;
            sh.codebook = (0..256).map(|i| (i as f32 - 128.) / 512.).collect();
            let pixels = (0..64 * 15)
                .map(|i| {
                    [
                        ((i * 7) % 256) as u8,
                        ((i * 13 + 1) % 256) as u8,
                        ((i * 17 + 3) % 256) as u8,
                        255,
                    ]
                })
                .collect::<Vec<_>>();
            files.insert("shN_centroids.webp".into(), webp(&pixels, 64 * 15, 1));
        }
        let packed = decode(
            &meta,
            &files,
            bands,
            DecodeLimits {
                max_texture_dimension: 4096,
                max_texture_bytes: 1 << 20,
            },
        )
        .unwrap();
        let source = Arc::new(WgpuWrapper::new(
            GpuSogSource::upload(
                device,
                queue,
                &layout,
                &packed,
                GpuSogSource::required_bytes(&packed),
            )
            .unwrap(),
        ));
        let weak_source = Arc::downgrade(&source);
        let make_selection =
            |work: &Arc<WgpuWrapper<CompactWork>>,
             specs: [(u32, u32, u32); 2],
             first_source: &Arc<WgpuWrapper<GpuSogSource>>| {
                let indirect = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16,
                    mapped_at_creation: false,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::INDIRECT
                        | wgpu::BufferUsages::COPY_DST,
                });
                let ranges = specs
                    .map(|(start, count, destination)| SourceRange {
                        source: if destination == 0 {
                            first_source.clone()
                        } else {
                            source.clone()
                        },
                        range: BakeRange {
                            source_start: start,
                            count,
                            destination_start: destination,
                        },
                    })
                    .into_iter()
                    .collect();
                let mut cloud = CompactCloud::from_sources(
                    work.clone(),
                    baker.clone(),
                    ranges,
                    indirect.into(),
                    Entity::PLACEHOLDER,
                )
                .unwrap();
                cloud.set_color_refresh_angle(10.0).unwrap();
                cloud
            };
        let make_cloud = |second_start, first_source: &Arc<WgpuWrapper<GpuSogSource>>| {
            let work = Arc::new(WgpuWrapper::new(CompactWork::new(device, 3, 72).unwrap()));
            let cloud = make_selection(&work, [(0, 2, 0), (second_start, 1, 2)], first_source);
            (cloud, work)
        };
        let diagnostics = BakeDiagnostics::enabled();
        let disabled = BakeDiagnostics::default();
        let mut state = BakeState::default();
        let (mut cloud, work) = make_cloud(2, &source);
        let a = LocalView::perspective([0., 0., 5.]).unwrap();
        let b = LocalView::perspective([0.0001, 0., 5.]).unwrap();
        let c = LocalView::perspective([10., 3., -5.]).unwrap();
        let run =
            |state: &mut BakeState, cloud: &mut CompactCloud, view, diag: &BakeDiagnostics| {
                let mut encoder = device.create_command_encoder(&Default::default());
                state.bake(cloud, view, device, || &mut encoder, diag);
                assert!(cloud.ready);
                queue.submit([encoder.finish()]);
            };
        run(&mut state, &mut cloud, a, &diagnostics);
        let initial = read_work(device, queue, &work);
        let report = diagnostics.report().unwrap();
        assert_eq!(
            (report.full_rows, report.color_rows, report.dispatches),
            (3, 0, 2),
            "instrument the actual initial two-range bake"
        );
        assert_eq!(
            (
                report.uniform_buffers,
                report.uniform_bytes,
                report.bind_groups,
                report.compute_passes
            ),
            (1, pair_bytes, 1, 1)
        );
        for recorder in [&diagnostics, &disabled] {
            state.bake(
                &mut cloud,
                a,
                device,
                || panic!("cache hit must not request a GPU encoder"),
                recorder,
            );
            assert!(cloud.ready);
        }
        assert_eq!(read_work(device, queue, &work), initial);
        assert_eq!(diagnostics.report().unwrap().cache_hits, 1);
        let move_view = |state: &mut BakeState, cloud: &mut CompactCloud, view| {
            if bands == 0 {
                state.bake(
                    cloud,
                    view,
                    device,
                    || panic!("SH0 motion must reuse colors without an encoder"),
                    &diagnostics,
                );
                assert!(cloud.ready);
            } else {
                run(state, cloud, view, &diagnostics);
            }
        };
        state.bake(
            &mut cloud,
            b,
            device,
            || panic!("sub-threshold motion must not request an encoder"),
            &diagnostics,
        );
        assert_eq!(
            diagnostics.report().unwrap().color_rows,
            0,
            "sub-threshold motion must retain already baked colors"
        );
        move_view(&mut state, &mut cloud, c);
        let moved = read_work(device, queue, &work);
        assert_eq!(moved[..2], initial[..2], "motion cannot rewrite geometry");
        if bands == 0 {
            assert_eq!(moved[2], initial[2]);
        } else {
            assert_ne!(
                moved[2], initial[2],
                "fixture must expose view-dependent SH3"
            );
        }
        let (mut replacement, replacement_work) = make_cloud(1, &source);
        run(&mut state, &mut replacement, c, &diagnostics);
        let replaced = read_work(device, queue, &replacement_work);
        for (plane, stride) in [4, 1, 1].into_iter().enumerate() {
            assert_eq!(
                &replaced[plane][..2 * stride],
                &moved[plane][..2 * stride],
                "unchanged range output must survive replacement"
            );
            assert_eq!(
                &replaced[plane][2 * stride..3 * stride],
                &moved[plane][stride..2 * stride]
            );
        }
        let report = diagnostics.report().unwrap();
        assert_eq!(
            (report.full_updates, report.color_updates, report.cache_hits),
            (2, 2, 1)
        );
        assert_eq!(
            (report.full_rows, report.color_rows, report.dispatches),
            if bands == 0 { (4, 0, 3) } else { (4, 3, 5) },
            "a replacement must decode only its changed range"
        );
        assert_eq!(
            (
                report.uniform_buffers,
                report.uniform_bytes,
                report.bind_groups,
                report.compute_passes
            ),
            if bands == 0 {
                (2, pair_bytes + 48, 2, 2)
            } else {
                (3, 2 * pair_bytes + 48, 3, 3)
            }
        );
        eprintln!(
            "bake-work SH{bands} current={}",
            serde_json::to_string(&report).unwrap()
        );

        // Exercise the actual slot-keyed production cache, including retirement
        // of the entity/source-generation description between slot uses.
        let mut cache = BakeCache::default();
        let cache_diag = BakeDiagnostics::enabled();
        let run_cached = |cache: &mut BakeCache, cloud: &mut CompactCloud, view| {
            let mut encoder = device.create_command_encoder(&Default::default());
            cache.bake(cloud, view, device, || &mut encoder, &cache_diag);
            assert!(cloud.ready);
            queue.submit([encoder.finish()]);
        };
        {
            let (_unbaked, unbaked_work) = make_cloud(2, &source);
            let (mut target, target_work) = make_cloud(1, &source);
            target.reuse_work_from(&unbaked_work).unwrap();
            let mut cold_cache = BakeCache::default();
            let cold_diag = BakeDiagnostics::enabled();
            let mut encoder = device.create_command_encoder(&Default::default());
            cold_cache.bake(&mut target, c, device, || &mut encoder, &cold_diag);
            queue.submit([encoder.finish()]);
            assert!(target.ready);
            assert_eq!(cold_diag.report().unwrap().full_rows, 3);
            assert_eq!(
                cold_diag.report().unwrap().copied_rows,
                0,
                "allocated but unbaked donor contains no reusable data"
            );
            assert_eq!(read_work(device, queue, &target_work), replaced);
        }
        let (mut first, slot_a) = make_cloud(2, &source);
        run_cached(&mut cache, &mut first, c);
        let (mut second, slot_b) = make_cloud(1, &source);
        second.reuse_work_from(&slot_a).unwrap();
        run_cached(&mut cache, &mut second, c);
        assert_eq!(read_work(device, queue, &slot_b), replaced);
        assert_eq!(cache_diag.report().unwrap().full_rows, 4);
        assert_eq!(cache_diag.report().unwrap().copied_rows, 2);
        drop(first);
        let specs = [(1, 1, 0), (0, 2, 1)];
        let mut reordered = make_selection(&slot_a, specs, &source);
        reordered.reuse_work_from(&slot_b).unwrap();
        run_cached(&mut cache, &mut reordered, c);
        let reordered_pixels = read_work(device, queue, &slot_a);
        for (plane, stride) in [4, 1, 1].into_iter().enumerate() {
            assert_eq!(
                &reordered_pixels[plane][..stride],
                &replaced[plane][2 * stride..]
            );
            assert_eq!(
                &reordered_pixels[plane][stride..],
                &replaced[plane][..2 * stride]
            );
        }
        assert_eq!(
            cache_diag.report().unwrap().full_rows,
            4,
            "moved ranges copy, not decode"
        );
        assert_eq!(cache_diag.report().unwrap().copied_rows, 5);
        drop(reordered);
        let mut retained = make_selection(&slot_a, specs, &source);
        cache.bake(
            &mut retained,
            c,
            device,
            || panic!("retired generation's unchanged slot contents remain usable"),
            &cache_diag,
        );
        assert!(retained.ready);
        assert_eq!(read_work(device, queue, &slot_a), reordered_pixels);

        // Same encoded bytes under a new immutable owner are not the same GPU
        // source identity. The cache must never mistake recycled file IDs for
        // valid decoded contents.
        let other_source = Arc::new(WgpuWrapper::new(
            GpuSogSource::upload(
                device,
                queue,
                &layout,
                &packed,
                GpuSogSource::required_bytes(&packed),
            )
            .unwrap(),
        ));
        drop(retained);
        let mut replaced_owner = make_selection(&slot_a, specs, &other_source);
        let full_before = cache_diag.report().unwrap().full_rows;
        run_cached(&mut cache, &mut replaced_owner, c);
        assert_eq!(
            cache_diag.report().unwrap().full_rows,
            full_before + 1,
            "only the range with a new source owner needs decoding"
        );
        assert_eq!(read_work(device, queue, &slot_a), reordered_pixels);
        drop(replaced_owner);
        drop((second, slot_a, slot_b));

        if bands > 0 {
            let (mut exact, exact_work) = make_cloud(2, &source);
            exact.set_color_refresh_angle(0.0).unwrap();
            let mut exact_state = BakeState::default();
            let exact_diag = BakeDiagnostics::enabled();
            run(&mut exact_state, &mut exact, a, &exact_diag);
            run(&mut exact_state, &mut exact, b, &exact_diag);
            assert_eq!(
                exact_diag.report().unwrap().color_rows,
                3,
                "zero threshold retains the exact-refresh oracle"
            );
            let (mut exact_oracle, exact_oracle_work) = make_cloud(2, &source);
            run(&mut BakeState::default(), &mut exact_oracle, b, &disabled);
            assert_eq!(
                read_work(device, queue, &exact_work),
                read_work(device, queue, &exact_oracle_work)
            );

            let (mut accumulated, accumulated_work) = make_cloud(2, &source);
            let accum_diag = BakeDiagnostics::enabled();
            let mut accum_state = BakeState::default();
            run(&mut accum_state, &mut accumulated, a, &accum_diag);
            // Small steps must accumulate, not postpone refresh forever.
            for i in 1..=100 {
                run(
                    &mut accum_state,
                    &mut accumulated,
                    LocalView::perspective([i as f32 * 0.02, 0., 5.]).unwrap(),
                    &accum_diag,
                );
            }
            let count = accum_diag.report().unwrap().color_rows;
            assert!(
                count > 0 && count < 300,
                "refresh selectively, without starvation"
            );
            let ortho = LocalView::orthographic([1., 0., 0.]).unwrap();
            run(&mut accum_state, &mut accumulated, ortho, &accum_diag);
            assert_eq!(accum_diag.report().unwrap().color_rows, count + 3);
            let (mut oracle, oracle_work) = make_cloud(2, &source);
            run(&mut BakeState::default(), &mut oracle, ortho, &disabled);
            assert_eq!(
                read_work(device, queue, &accumulated_work),
                read_work(device, queue, &oracle_work)
            );
        }

        let (mut unobserved, unobserved_work) = make_cloud(1, &source);
        let mut unobserved_state = BakeState::default();
        run(&mut unobserved_state, &mut unobserved, c, &disabled);
        assert_eq!(
            read_work(device, queue, &unobserved_work),
            replaced,
            "instrumentation must not change output"
        );
        assert!(disabled.report().is_none());
        if bands == 0 {
            // Projection-mode/direction changes cannot affect SH0 either.
            for view in [
                LocalView::orthographic([1., 0., 0.]).unwrap(),
                LocalView::orthographic([0., 1., 0.]).unwrap(),
                a,
            ] {
                unobserved_state.bake(
                    &mut unobserved,
                    view,
                    device,
                    || panic!("SH0 projection changes must not request an encoder"),
                    &disabled,
                );
                assert!(unobserved.ready);
                assert_eq!(read_work(device, queue, &unobserved_work), replaced);
            }
        } else {
            // A mixed generation must still refresh SH3, without touching SH0.
            let sh0_packed = decode(
                &meta,
                &files,
                0,
                DecodeLimits {
                    max_texture_dimension: 4096,
                    max_texture_bytes: 1 << 20,
                },
            )
            .unwrap();
            let sh0 = Arc::new(WgpuWrapper::new(
                GpuSogSource::upload(
                    device,
                    queue,
                    &layout,
                    &sh0_packed,
                    GpuSogSource::required_bytes(&sh0_packed),
                )
                .unwrap(),
            ));
            let (mut mixed, mixed_work) = make_cloud(2, &sh0);
            let mixed_diag = BakeDiagnostics::enabled();
            let mut mixed_state = BakeState::default();
            run(&mut mixed_state, &mut mixed, a, &mixed_diag);
            let before = read_work(device, queue, &mixed_work);
            run(&mut mixed_state, &mut mixed, c, &mixed_diag);
            let after = read_work(device, queue, &mixed_work);
            assert_eq!(after[..2], before[..2]);
            assert_eq!(after[2][..2], before[2][..2], "SH0 range is constant");
            assert_ne!(after[2][2], before[2][2], "SH3 range must track camera");
            let report = mixed_diag.report().unwrap();
            assert_eq!(
                (report.full_rows, report.color_rows, report.dispatches),
                (3, 1, 3)
            );
            // Compare reused output against a fresh full bake at the same view.
            let (mut oracle, oracle_work) = make_cloud(2, &sh0);
            run(&mut BakeState::default(), &mut oracle, c, &disabled);
            assert_eq!(after, read_work(device, queue, &oracle_work));
        }
        drop((cloud, replacement, unobserved, source));
        assert!(
            weak_source.upgrade().is_none(),
            "cache/diagnostics cannot pin source owners"
        );
    }
}

#[test]
#[ignore = "requires native GPU; bounded batch and deferred camera parameters"]
fn packed_bake_batches_preserve_sources_and_deferred_views() {
    bake_batch_probe(false);
}

#[test]
#[ignore = "requires native GPU; device without dynamic uniform bindings"]
fn packed_bake_batches_fall_back_without_dynamic_uniforms() {
    bake_batch_probe(true);
}

fn bake_batch_probe(without_dynamic_uniforms: bool) {
    use crate::render::{
        compact::CompactCloud,
        sog::{
            diagnostics::BakeDiagnostics,
            view::{BakeState, SourceRange},
        },
    };
    use bevy::{prelude::Entity, render::renderer::WgpuWrapper};
    use std::sync::Arc;
    let mut settings = bevy::render::settings::WgpuSettings {
        priority: bevy::render::settings::WgpuSettingsPriority::WebGPU,
        ..Default::default()
    };
    if without_dynamic_uniforms {
        settings
            .limits
            .max_dynamic_uniform_buffers_per_pipeline_layout = 0;
    }
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &settings,
    ));
    eprintln!("batch probe adapter={:?}", resources.2.0);
    let device = resources.0.wgpu_device();
    assert_eq!(
        device
            .limits()
            .max_dynamic_uniform_buffers_per_pipeline_layout
            == 0,
        without_dynamic_uniforms
    );
    let queue = &resources.1;
    let layout = GpuSogSource::layout(device);
    let baker = Arc::new(WgpuWrapper::new(CompactBaker::new(device).unwrap()));
    let sources = [[127, 127, 127, 204], [180, 100, 210, 204]].map(|color| {
        let (meta, mut files) = fixture(3, true);
        files.insert("sh0.webp".into(), webp(&[color; 4], 2, 2));
        let packed = decode(
            &meta,
            &files,
            1,
            DecodeLimits {
                max_texture_dimension: 4096,
                max_texture_bytes: 1 << 20,
            },
        )
        .unwrap();
        Arc::new(WgpuWrapper::new(
            GpuSogSource::upload(
                device,
                queue,
                &layout,
                &packed,
                GpuSogSource::required_bytes(&packed),
            )
            .unwrap(),
        ))
    });
    let work = Arc::new(WgpuWrapper::new(
        CompactWork::new(device, 130, 130 * 24).unwrap(),
    ));
    let before_copy = CompactWork::new(device, 130, 130 * 24).unwrap();
    let indirect = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        mapped_at_creation: false,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::COPY_DST,
    });
    let ranges = (0..130)
        .map(|i| SourceRange {
            source: sources[i as usize % 2].clone(),
            range: BakeRange {
                source_start: i % 3,
                count: 1,
                destination_start: i,
            },
        })
        .collect();
    let mut cloud = CompactCloud::from_sources(
        work.clone(),
        baker.clone(),
        ranges,
        indirect.into(),
        Entity::PLACEHOLDER,
    )
    .unwrap();
    let mut state = BakeState::default();
    let diagnostics = BakeDiagnostics::enabled();
    let a = LocalView::perspective([0., 0., 5.]).unwrap();
    let b = LocalView::perspective([10., 3., -5.]).unwrap();
    let mut first = device.create_command_encoder(&Default::default());
    state.bake(&mut cloud, a, device, || &mut first, &diagnostics);
    assert!(cloud.ready);
    for (src, dst) in work.buffers().into_iter().zip(before_copy.buffers()) {
        first.copy_buffer_to_buffer(src, 0, dst, 0, src.size());
    }
    let mut second = device.create_command_encoder(&Default::default());
    state.bake(&mut cloud, b, device, || &mut second, &diagnostics);
    assert!(cloud.ready);
    // Neither view has been submitted yet. Parameters must remain independent.
    queue.submit([first.finish(), second.finish()]);
    let before = read_work(device, queue, &before_copy);
    let after = read_work(device, queue, &work);
    assert_eq!(before[..2], after[..2]);
    assert_ne!(
        before[2], after[2],
        "SH fixture must expose view dependence"
    );
    for (view, actual) in [(a, &before), (b, &after)] {
        let expected = sources.each_ref().map(|source| {
            let oracle = CompactWork::new(device, 3, 72).unwrap();
            let mut encoder = device.create_command_encoder(&Default::default());
            baker
                .encode(
                    device,
                    &mut encoder,
                    source,
                    &oracle,
                    BakeRange {
                        source_start: 0,
                        count: 3,
                        destination_start: 0,
                    },
                    view,
                    false,
                )
                .unwrap();
            queue.submit([encoder.finish()]);
            read_work(device, queue, &oracle)
        });
        for i in 0..130 {
            for (plane, stride) in [4, 1, 1].into_iter().enumerate() {
                assert_eq!(
                    &actual[plane][i * stride..(i + 1) * stride],
                    &expected[i % 2][plane][(i % 3) * stride..(i % 3 + 1) * stride],
                    "range {i} plane {plane} must retain its source, offset and view"
                );
            }
        }
    }
    let report = diagnostics.report().unwrap();
    assert_eq!(
        (report.full_rows, report.color_rows, report.dispatches),
        (130, 130, 260)
    );
    assert_eq!(
        (
            report.uniform_buffers,
            report.bind_groups,
            report.compute_passes
        ),
        if without_dynamic_uniforms {
            (260, 260, 260)
        } else {
            (6, 6, 6)
        },
        "130 ranges split into three bounded batches for each view"
    );
    assert!(report.uniform_bytes <= 6 * 16384);
    eprintln!("batch-work {}", serde_json::to_string(&report).unwrap());
}

fn compact_ranges_and_color_refresh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &GpuSogSource,
    baker: &CompactBaker,
    decoded: &[[f32; 4]],
) {
    assert!(CompactWork::new(device, 5, 119).is_err());
    let work = CompactWork::new(device, 5, 120).expect("24 bytes per slot");
    let mut encoder = device.create_command_encoder(&Default::default());
    // Two non-contiguous source ranges; multiple dispatches in one submission
    // must not accidentally all read the last range's uniform parameters.
    for range in [
        BakeRange {
            source_start: 2,
            count: 1,
            destination_start: 1,
        },
        BakeRange {
            source_start: 0,
            count: 2,
            destination_start: 3,
        },
    ] {
        baker
            .encode(
                device,
                &mut encoder,
                source,
                &work,
                range,
                LocalView::orthographic([1.0, 0.0, 0.0]).unwrap(),
                false,
            )
            .unwrap();
    }
    queue.submit([encoder.finish()]);
    let before = read_work(device, queue, &work);
    for empty in [0, 2] {
        assert_eq!(&before[0][empty * 4..empty * 4 + 4], &[0; 4]);
        assert_eq!(before[1][empty], 0);
        assert_eq!(before[2][empty], 0);
    }
    for (input, destination) in [(2, 1), (0, 3), (1, 4)] {
        for axis in 0..3 {
            assert_eq!(
                before[0][destination * 4 + axis],
                decoded[input * 22][axis].to_bits()
            );
        }
        let packed = before[0][destination * 4 + 3];
        assert_eq!(packed >> 24, 204);
        // A log-quantization bin is 21/255; half-bin relative scale error <= 4.3%.
        for axis in 0..3 {
            let code = (packed >> (axis * 8)) & 255;
            let actual = (code as f32 * 21.0 / 255.0 - 12.0).exp();
            assert!((actual / decoded[input * 22 + 2][axis] - 1.0).abs() < 0.043);
        }
        let q = before[1][destination];
        let p = bevy::math::Vec3::new(
            (q & 2047) as f32 / 2047.0 * 2.0 - 1.0,
            ((q >> 11) & 2047) as f32 / 2047.0 * 2.0 - 1.0,
            ((q >> 22) & 1023) as f32 / 1023.0 * 2.0 - 1.0,
        );
        let len = p.length_squared();
        let xyz = (2.0 - len).max(0.0).sqrt() * p;
        let restored = [xyz.x, xyz.y, xyz.z, 1.0 - len];
        let dot: f32 = restored
            .iter()
            .zip(decoded[input * 22 + 1])
            .map(|(a, b)| a * b)
            .sum();
        assert!((dot.abs() - 1.0).abs() < 0.00002);
        check_compact_color(before[2][destination], decoded[input * 22 + 3]);
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    baker
        .encode(
            device,
            &mut encoder,
            source,
            &work,
            BakeRange {
                source_start: 0,
                count: 2,
                destination_start: 3,
            },
            LocalView::orthographic([0.0, 3.0, 0.0]).unwrap(),
            true,
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let after = read_work(device, queue, &work);
    assert_eq!(
        before[0], after[0],
        "color refresh must not rewrite geometry"
    );
    assert_eq!(
        before[1], after[1],
        "color refresh must not rewrite rotation"
    );
    assert_eq!(
        before[2][..3],
        after[2][..3],
        "only changed range may update"
    );
    for (input, destination) in [(0, 3), (1, 4)] {
        check_compact_color(after[2][destination], decoded[input * 22 + 4]);
    }
    // A perspective camera one unit behind the first splat must use the same
    // +X source-space ray as the first orthographic bake, even after moving.
    let position = decoded[0];
    let mut encoder = device.create_command_encoder(&Default::default());
    baker
        .encode(
            device,
            &mut encoder,
            source,
            &work,
            BakeRange {
                source_start: 0,
                count: 1,
                destination_start: 3,
            },
            LocalView::perspective([position[0] - 1.0, position[1], position[2]]).unwrap(),
            true,
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let perspective = read_work(device, queue, &work);
    check_compact_color(perspective[2][3], decoded[3]);
    assert_eq!(perspective[0], before[0]);

    let mut encoder = device.create_command_encoder(&Default::default());
    baker
        .encode(
            device,
            &mut encoder,
            source,
            &work,
            BakeRange {
                source_start: 0,
                count: 1,
                destination_start: 3,
            },
            LocalView::perspective([position[0], position[1], position[2]]).unwrap(),
            true,
        )
        .unwrap();
    queue.submit([encoder.finish()]);
    let at_center = read_work(device, queue, &work);
    let mut expected = [0.5; 4];
    for (c, value) in basis([0.0, 0.0, 1.0]).into_iter().enumerate() {
        for channel in 0..3 {
            expected[channel] += value * decoded[6 + c][channel];
        }
    }
    check_compact_color(at_center[2][3], expected);
}

fn check_compact_color(packed: u32, expected: [f32; 4]) {
    let actual = [
        (packed & 2047) as f32 * 4.0 / 2047.0,
        ((packed >> 11) & 2047) as f32 * 4.0 / 2047.0,
        ((packed >> 22) & 1023) as f32 * 4.0 / 1023.0,
    ];
    for channel in 0..3 {
        assert!(
            (actual[channel] - expected[channel].clamp(0.0, 4.0)).abs() < 0.002,
            "quantized RGB: actual={actual:?}, expected={expected:?}"
        );
    }
}
