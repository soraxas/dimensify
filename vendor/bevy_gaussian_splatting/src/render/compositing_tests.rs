//! Actual Gaussian raster overlap readback. Output encoding must not relabel
//! the input SH colour space or change the ordinary linear-view pipeline.
use super::*;
use bevy::render::renderer::RenderQueue;
use wgpu::util::DeviceExt;

#[test]
#[ignore = "headless native GPU correctness experiment; run with --ignored --nocapture"]
fn production_raster_overlap_isolates_linear_and_encoded_compositing() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    eprintln!("compositing adapter={:?}", resources.2.0);
    let (device, queue) = (resources.0, resources.1);
    for (reverse, input_linear) in [(false, false), (true, false), (false, true), (true, true)] {
        // Two coincident-centre half-opaque splats, over an opaque background.
        // The centre pixel's normalized Gaussian alpha is exactly .5, so the
        // front/back/background weights are .5/.25/.25. Reversing the actual
        // sort entries swaps front/back; no projection or sorting guess is used.
        let linear = overlap(&device, &queue, false, reverse, input_linear, false);
        let encoded = overlap(&device, &queue, true, reverse, input_linear, false);
        let expected_linear = if reverse {
            [0.17001492, 0.32849960, 0.34292393, 1.]
        } else {
            [0.31269556, 0.18581896, 0.38934354, 1.]
        };
        let expected_encoded = if reverse {
            [0.325, 0.525, 0.575, 1.]
        } else {
            [0.475, 0.375, 0.625, 1.]
        };
        // Decode the complete reference-space composite exactly once before
        // returning it to a linear Bevy view. These hand-calculated values
        // differ from decoding each splat before alpha blending.
        let expected_encoded_to_linear = if reverse {
            [0.08623324, 0.23791470, 0.29014254]
        } else {
            [0.19162677, 0.11601613, 0.34851020]
        };
        for channel in 0..4 {
            assert!(
                (linear[channel] - expected_linear[channel]).abs() < 0.001,
                "linear reverse={reverse}: {linear:?}"
            );
            assert!(
                (encoded[channel] - expected_encoded[channel]).abs() < 0.001,
                "encoded reverse={reverse}: {encoded:?}"
            );
        }
        for channel in 0..3 {
            let decoded = decode_srgb(encoded[channel]);
            assert!((decoded - expected_encoded_to_linear[channel]).abs() < 0.001);
            assert!(
                (linear[channel] - decoded).abs() > 0.035,
                "overlap fixture must distinguish blend space, channel={channel}"
            );
        }
        eprintln!(
            "reverse={reverse}, input_linear={input_linear}, linear={linear:?}, encoded={encoded:?}"
        );
    }
}

#[test]
#[ignore = "headless native GPU correctness; run with --ignored --nocapture"]
fn production_gamma_roundtrip_composites_without_relabeling_input() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    for input_linear in [false, true] {
        for (reverse, expected) in [
            (false, [0.19162677, 0.11601613, 0.34851020, 1.]),
            (true, [0.08623324, 0.23791470, 0.29014254, 1.]),
        ] {
            let actual = overlap(
                &resources.0,
                &resources.1,
                true,
                reverse,
                input_linear,
                true,
            );
            for channel in 0..4 {
                assert!(
                    (actual[channel] - expected[channel]).abs() < 0.001,
                    "full roundtrip, reverse={reverse}, linear_input={input_linear}: {actual:?}"
                );
            }
        }
    }
}

fn decode_srgb(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn overlap(
    device: &RenderDevice,
    queue: &RenderQueue,
    encoded_target: bool,
    reverse: bool,
    input_linear: bool,
    roundtrip: bool,
) -> [f32; 4] {
    let raw = device.wgpu_device();
    let module = super::raster_profile_tests::production_module(
        CloudPipelineKey {
            raster_profile: GaussianRasterProfile::NormalizedGaussian,
            output_srgb: encoded_target,
            ..default()
        },
        false,
    );
    let identity = Mat4::IDENTITY.to_cols_array();
    let projection =
        Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1., 0.1).to_cols_array();
    let view_bytes = super::frustum_tests::uniform_bytes(
        &module,
        0,
        &[
            ("viewport", bytemuck::cast_slice(&[0f32, 0., 65., 65.])),
            ("view_from_world", bytemuck::cast_slice(&identity)),
            ("world_from_view", bytemuck::cast_slice(&identity)),
            ("clip_from_view", bytemuck::cast_slice(&projection)),
            (
                "unjittered_clip_from_world",
                bytemuck::cast_slice(&projection),
            ),
        ],
    );
    let cloud_bytes = super::frustum_tests::uniform_bytes(
        &module,
        1,
        &[
            ("transform", bytemuck::cast_slice(&identity)),
            ("global_scale", bytemuck::bytes_of(&1f32)),
            ("global_opacity", bytemuck::bytes_of(&1f32)),
            ("count", bytemuck::bytes_of(&2u32)),
            // Either input representation describes the same straight RGB.
            // Changing the target must never change the input interpretation.
            ("color_space", bytemuck::bytes_of(&u32::from(input_linear))),
        ],
    );
    let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production Gaussian overlap"),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
    });
    let pipeline = raw.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("isolated overlap diagnostic"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_points"),
            buffers: &[],
            compilation_options: default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..default()
        },
        depth_stencil: None,
        multisample: default(),
        multiview_mask: None,
        cache: None,
    });
    let make = |bytes: &[u8], usage| {
        raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytes,
            usage,
        })
    };
    let bind = |group, buffers: &[&wgpu::Buffer]| {
        raw.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(group),
            entries: &buffers
                .iter()
                .enumerate()
                .map(|(binding, b)| wgpu::BindGroupEntry {
                    binding: binding as u32,
                    resource: b.as_entire_binding(),
                })
                .collect::<Vec<_>>(),
        })
    };
    let view = make(&view_bytes, wgpu::BufferUsages::UNIFORM);
    let cloud = make(&cloud_bytes, wgpu::BufferUsages::UNIFORM);
    let positions = make(
        bytemuck::cast_slice(&[0f32, 0., -3., 1., 0., 0., -2., 1.]),
        wgpu::BufferUsages::STORAGE,
    );
    let mut sh = vec![0f32; SH_COEFF_COUNT * 2];
    for (index, color) in [[0.2, 0.8, 0.4], [0.8, 0.2, 0.6]].iter().enumerate() {
        for channel in 0..3 {
            let value = if input_linear {
                decode_srgb(color[channel])
            } else {
                color[channel]
            };
            sh[index * SH_COEFF_COUNT + channel] = (value - 0.5) / 0.2820948;
        }
    }
    let sh = make(bytemuck::cast_slice(&sh), wgpu::BufferUsages::STORAGE);
    let rotations = make(
        bytemuck::cast_slice(&[1f32, 0., 0., 0., 1., 0., 0., 0.]),
        wgpu::BufferUsages::STORAGE,
    );
    let scales = make(
        bytemuck::cast_slice(&[0.4f32, 0.4, 0.4, 0.5, 0.4, 0.4, 0.4, 0.5]),
        wgpu::BufferUsages::STORAGE,
    );
    let entries = if reverse {
        [0u32, 1, 0, 0]
    } else {
        [0u32, 0, 0, 1]
    };
    let sorted = make(bytemuck::cast_slice(&entries), wgpu::BufferUsages::STORAGE);
    let groups = [
        bind(0, &[&view]),
        bind(1, &[&cloud]),
        bind(2, &[&positions, &sh, &rotations, &scales]),
        bind(3, &[&sorted]),
    ];
    let target = raw.create_texture(&wgpu::TextureDescriptor {
        label: Some("isolated colour-space target"),
        size: wgpu::Extent3d {
            width: 65,
            height: 65,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let intermediate = raw.create_texture(&wgpu::TextureDescriptor {
        label: Some("encoded intermediate"),
        size: target.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: target.format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let staging = raw.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut background = [0.1, 0.3, 0.9];
    if !encoded_target || roundtrip {
        background = background.map(decode_srgb);
    }
    let target_view = target.create_view(&default());
    let intermediate_view = intermediate.create_view(&default());
    let mut encoder = device.create_command_encoder(&default());
    let clear = wgpu::Color {
        r: background[0] as f64,
        g: background[1] as f64,
        b: background[2] as f64,
        a: 1.,
    };
    if roundtrip {
        // The real opaque/background colour is already linear. Encode it on
        // the GPU before the splat draw, and decode the complete result below.
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..default()
            });
        }
        record_transfer(
            device,
            &mut encoder,
            &target_view,
            &intermediate_view,
            "encode",
        );
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: if roundtrip {
                    &intermediate_view
                } else {
                    &target_view
                },
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if roundtrip {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(clear)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..default()
        });
        pass.set_pipeline(&pipeline);
        for (index, group) in groups.iter().enumerate() {
            pass.set_bind_group(index as u32, group, &[]);
        }
        pass.draw(0..4, 0..2);
    }
    if roundtrip {
        record_transfer(
            device,
            &mut encoder,
            &intermediate_view,
            &target_view,
            "decode",
        );
    }
    let mut copy = target.as_image_copy();
    copy.origin = wgpu::Origin3d { x: 32, y: 32, z: 0 };
    encoder.copy_texture_to_buffer(
        copy,
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll");
    receive.recv().expect("readback callback").expect("GPU map");
    let data = staging.slice(..).get_mapped_range();
    let result = std::array::from_fn(|i| {
        half::f16::from_bits(u16::from_le_bytes([data[2 * i], data[2 * i + 1]])).to_f32()
    });
    drop(data);
    staging.unmap();
    result
}

fn record_transfer(
    device: &RenderDevice,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::TextureView,
    destination: &wgpu::TextureView,
    entry: &str,
) {
    let raw = device.wgpu_device();
    let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production transfer in raster roundtrip"),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(
            super::raster_profile_tests::production_module_source(
                CloudPipelineKey::default(),
                include_str!("compositing_transfer.wgsl").into(),
            ),
        )),
    });
    let pipeline = raw.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[],
            compilation_options: default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(entry),
            compilation_options: default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: default(),
        depth_stencil: None,
        multisample: default(),
        multiview_mask: None,
        cache: None,
    });
    let rect = raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[0u32, 0, 65, 65]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bind = raw.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: rect.as_entire_binding(),
            },
        ],
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: destination,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..default()
    });
    pass.set_pipeline(&pipeline);
    pass.set_bind_group(0, &bind, &[]);
    pass.draw(0..3, 0..1);
}
