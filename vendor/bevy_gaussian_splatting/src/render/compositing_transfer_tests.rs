//! Actual transfer shader readback, including alpha, HDR and viewport isolation.
use super::*;
use wgpu::util::DeviceExt;

#[test]
#[ignore = "headless native GPU correctness; run with --ignored --nocapture"]
fn production_compositing_transfer_preserves_alpha_hdr_and_other_viewports() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    eprintln!("transfer adapter={:?}", resources.2.0);
    let device = &resources.0;
    let queue = &resources.1;
    let raw = device.wgpu_device();
    let module = super::raster_profile_tests::production_module_source(
        CloudPipelineKey::default(),
        include_str!("compositing_transfer.wgsl").into(),
    );
    let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("production colour transfer"),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
    });
    let layout = raw.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = raw.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let rect = raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[1u32, 0, 3, 2]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    // Only the middle two columns belong to the view. The background is
    // premultiplied; encoding RGB directly would fail the half-alpha row.
    let input: [[f32; 4]; 8] = [
        [0.18, 0.4, 1., 0.5],
        [0.125, 0.25, 0.5, 0.5],
        [2., -0.01, 0.0031308, 1.],
        [0.9, 0.2, 0.1, 1.],
        [0.2, 0.8, 0.4, 1.],
        [0., 0., 0., 0.],
        [0.0005, 0.001, 0.002, 0.25],
        [0.33, 0.01, 0.8, 0.75],
    ];
    let expected = [
        input[0],
        [0.26854935, 0.36767849, 0.5, 0.5],
        [1.35325605, -0.1292, 0.04044994, 1.],
        input[3],
        input[4],
        [0., 0., 0., 0.],
        [0.00646, 0.01267718, 0.02152605, 0.25],
        input[7],
    ];
    for format in [
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::Rgba32Float,
    ] {
        let half = format == wgpu::TextureFormat::Rgba16Float;
        let pixel_bytes = if half { 8 } else { 16 };
        let bytes: Vec<u8> = input
            .iter()
            .flatten()
            .flat_map(|v| {
                if half {
                    half::f16::from_f32(*v).to_le_bytes().to_vec()
                } else {
                    v.to_le_bytes().to_vec()
                }
            })
            .collect();
        let textures = std::array::from_fn::<_, 3, _>(|_| {
            raw.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 4,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        });
        queue.write_texture(
            textures[0].as_image_copy(),
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * pixel_bytes),
                rows_per_image: Some(2),
            },
            wgpu::Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            },
        );
        let staging = raw.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let views = textures
            .each_ref()
            .map(|texture| texture.create_view(&default()));
        let mut encoder = device.create_command_encoder(&default());
        for (i, entry) in ["encode", "decode"].into_iter().enumerate() {
            let pipeline = raw.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
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
                        format,
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
            let bind = raw.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&views[i]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: rect.as_entire_binding(),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &views[i + 1],
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
            encoder.copy_texture_to_buffer(
                textures[i + 1].as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: i as u64 * 512,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(2),
                    },
                },
                wgpu::Extent3d {
                    width: 4,
                    height: 2,
                    depth_or_array_layers: 1,
                },
            );
        }
        queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = send.send(r);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("GPU poll");
        receive.recv().expect("callback").expect("GPU map");
        let data = staging.slice(..).get_mapped_range();
        for stage in 0..2 {
            for pixel in 0..8 {
                let offset = stage * 512 + pixel / 4 * 256 + pixel % 4 * pixel_bytes as usize;
                for channel in 0..4 {
                    let at = offset + channel * if half { 2 } else { 4 };
                    let actual = if half {
                        half::f16::from_le_bytes(data[at..at + 2].try_into().expect("half"))
                            .to_f32()
                    } else {
                        f32::from_le_bytes(data[at..at + 4].try_into().expect("float"))
                    };
                    let want = if stage == 0 {
                        expected[pixel][channel]
                    } else {
                        input[pixel][channel]
                    };
                    let tolerance = if half { 0.003 } else { 0.00001 };
                    assert!(
                        (actual - want).abs() < tolerance,
                        "{format:?}, stage={stage}, pixel={pixel}, channel={channel}: {actual} != {want}"
                    );
                }
                if [0, 3, 4, 7].contains(&pixel) {
                    let begin = pixel * pixel_bytes as usize;
                    assert_eq!(
                        &data[offset..offset + pixel_bytes as usize],
                        &bytes[begin..begin + pixel_bytes as usize],
                        "other viewport must remain bit-exact"
                    );
                }
            }
        }
        drop(data);
        staging.unmap();
    }
}
