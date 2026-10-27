//! Rasterize the real vertex/fragment shader: centre-only visibility is not an oracle.
use super::*;
use wgpu::util::DeviceExt;

pub(super) fn uniform_bytes(
    module: &wgpu::naga::Module,
    group: u32,
    fields: &[(&str, &[u8])],
) -> Vec<u8> {
    let global = module
        .global_variables
        .iter()
        .map(|(_, v)| v)
        .find(|v| {
            v.binding
                .as_ref()
                .is_some_and(|b| b.group == group && b.binding == 0)
        })
        .expect("production uniform");
    let wgpu::naga::TypeInner::Struct { members, span } = &module.types[global.ty].inner else {
        panic!("uniform must be a struct");
    };
    let mut bytes = vec![0; *span as usize];
    for (name, value) in fields {
        let member = members
            .iter()
            .find(|m| m.name.as_deref() == Some(name))
            .expect("uniform field");
        let offset = member.offset as usize;
        bytes[offset..offset + value.len()].copy_from_slice(value);
    }
    bytes
}

#[test]
#[ignore = "headless native GPU correctness test; run explicitly with --ignored --nocapture"]
fn production_raster_preserves_offscreen_footprints_and_near_plane_splats() {
    raster_cases(
        Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1),
        &[
            ("centre", [0.0, 0.0, -2.0, 1.0], true),
            ("right footprint", [2.4, 0.0, -2.0, 1.0], true),
            ("left footprint", [-2.4, 0.0, -2.0, 1.0], true),
            ("upper footprint", [0.0, 2.4, -2.0, 1.0], true),
            ("lower footprint", [0.0, -2.4, -2.0, 1.0], true),
            ("before near plane", [0.0, 0.0, -0.05, 1.0], true),
            ("behind camera", [0.0, 0.0, 2.0, 1.0], false),
            ("outside footprint", [20.0, 0.0, -2.0, 1.0], false),
        ],
    );
}

#[test]
#[ignore = "headless native GPU correctness test; run explicitly with --ignored --nocapture"]
fn production_raster_bounds_near_eye_offscreen_kernels() {
    // Near-eye projection derivatives can make a remote centre's quad cover the
    // entire view. The reference caps kernel axes to the smaller viewport span:
    // centres at NDC +/-100 cannot reach this viewport after that cap. Nearby
    // edge-overlapping centres and a genuine on-axis near-eye splat must survive.
    raster_cases(
        Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1),
        &[
            ("near eye right", [0.1, 0.0, -0.001, 1.0], false),
            ("near eye left", [-0.1, 0.0, -0.001, 1.0], false),
            ("near eye above", [0.0, 0.1, -0.001, 1.0], false),
            ("near eye below", [0.0, -0.1, -0.001, 1.0], false),
            ("near eye centred", [0.0, 0.0, -0.001, 1.0], true),
            ("right overlapping", [2.4, 0.0, -2.0, 1.0], true),
        ],
    );
}

#[test]
#[ignore = "headless native GPU correctness test; run explicitly with --ignored --nocapture"]
fn production_orthographic_footprint_is_depth_independent_and_zoomable() {
    let cases = [
        ("near", [0.0, 0.0, -1.0, 1.0], true),
        ("far", [0.0, 0.0, -10.0, 1.0], true),
        ("farther", [0.0, 0.0, -100.0, 1.0], true),
        // Orthographic projection has no eye-plane singularity.
        ("eye plane", [0.0, 0.0, 0.0, 1.0], true),
        ("off-axis near", [0.25, 0.25, -1.0, 1.0], true),
        ("off-axis far", [0.25, 0.25, -100.0, 1.0], true),
    ];
    let ortho = raster_cases(
        Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 1000.0, 0.0),
        &cases,
    );
    let zoomed_out = raster_cases(
        Mat4::orthographic_rh(-4.0, 4.0, -4.0, 4.0, 1000.0, 0.0),
        &cases,
    );
    let coverage = |image: &[u8]| image.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
    for (original, zoomed) in ortho.iter().zip(&zoomed_out) {
        for images in [&original.images, &zoomed.images] {
            for image in &images[1..4] {
                assert!(
                    image == &images[0],
                    "depth changed ortho footprint: {:?}, aabb={}",
                    original.profile,
                    original.aabb
                );
            }
            assert!(
                images[4] == images[5],
                "off-axis ortho projection must not introduce perspective shear"
            );
            assert!(
                images[0] != images[4],
                "ortho centres must still move on screen"
            );
        }
        assert!(
            coverage(&original.images[0]) > 2 * coverage(&zoomed.images[0]),
            "doubling the visible ortho span must substantially shrink the footprint"
        );
    }
    // Guard against fixing ortho by dropping depth scaling in perspective too.
    let perspective = raster_cases(
        Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1),
        &cases[..2],
    );
    for result in perspective {
        assert!(
            coverage(&result.images[0]) > 2 * coverage(&result.images[1]),
            "perspective must still shrink distant splats"
        );
    }
}

struct RasterReadback {
    profile: GaussianRasterProfile,
    aabb: bool,
    images: Vec<Vec<u8>>,
}

fn raster_cases(projection: Mat4, cases: &[(&str, [f32; 4], bool)]) -> Vec<RasterReadback> {
    raster_cases_with_shape(
        projection,
        cases,
        [0.9238795, 0.0, 0.0, 0.38268343],
        [0.4, 0.3, 0.2, 1.0],
    )
}

#[test]
#[ignore = "headless native GPU correctness test; run explicitly with --ignored --nocapture"]
fn production_raster_preserves_axis_aligned_and_isotropic_splats() {
    for (label, scale, major_axis) in [
        ("horizontal", [0.4, 0.1, 0.2, 1.0], 1),
        ("vertical", [0.1, 0.4, 0.2, 1.0], -1),
        ("circular", [0.3, 0.3, 0.3, 1.0], 0),
    ] {
        for projection in [
            Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1),
            Mat4::orthographic_rh(-2.0, 2.0, -2.0, 2.0, 1000.0, 0.0),
        ] {
            let results = raster_cases_with_shape(
                projection,
                &[(label, [0.0, 0.0, -2.0, 1.0], true)],
                [1.0, 0.0, 0.0, 0.0],
                scale,
            );
            for result in results {
                // Measure visible alpha, not merely a finite CPU eigenvector.
                let image = &result.images[0];
                let mut min = [64usize; 2];
                let mut max = [0usize; 2];
                for (index, pixel) in image.chunks_exact(4).enumerate() {
                    if pixel[3] > 8 {
                        let position = [index % 64, index / 64];
                        for axis in 0..2 {
                            min[axis] = min[axis].min(position[axis]);
                            max[axis] = max[axis].max(position[axis]);
                        }
                    }
                }
                assert!(
                    min[0] < 64,
                    "visible kernel required: {label}, {:?}",
                    result.profile
                );
                let width = max[0] - min[0] + 1;
                let height = max[1] - min[1] + 1;
                assert!(
                    match major_axis {
                        1 => width > 2 * height,
                        -1 => height > 2 * width,
                        _ => width == height,
                    },
                    "{label}, {:?}, aabb={}: {width}x{height}",
                    result.profile,
                    result.aabb
                );
            }
        }
    }
}

fn raster_cases_with_shape(
    projection: Mat4,
    cases: &[(&str, [f32; 4], bool)],
    rotation_wxyz: [f32; 4],
    scale_opacity: [f32; 4],
) -> Vec<RasterReadback> {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        bevy::render::settings::Backends::all(),
        None,
        &bevy::render::settings::WgpuSettings::default(),
    ));
    eprintln!("offscreen correctness adapter={:?}", resources.2.0);
    let device = resources.0;
    let queue = resources.1;
    let raw = device.wgpu_device();
    let make = |bytes: &[u8], usage| {
        raw.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytes,
            usage,
        })
    };
    let target = raw.create_texture(&wgpu::TextureDescriptor {
        label: Some("Gaussian footprint regression"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&default());
    let staging = raw.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut failures = Vec::new();
    let mut results = Vec::new();
    for profile in [
        GaussianRasterProfile::Legacy,
        GaussianRasterProfile::NormalizedGaussian,
    ] {
        for aabb in [false, true] {
            let module = super::raster_profile_tests::production_module(
                CloudPipelineKey {
                    raster_profile: profile,
                    aabb,
                    ..default()
                },
                false,
            );
            let view_bytes = uniform_bytes(
                &module,
                0,
                &[
                    ("viewport", bytemuck::cast_slice(&[0.0f32, 0.0, 64.0, 64.0])),
                    (
                        "view_from_world",
                        bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
                    ),
                    (
                        "world_from_view",
                        bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
                    ),
                    (
                        "clip_from_view",
                        bytemuck::cast_slice(&projection.to_cols_array()),
                    ),
                    (
                        "unjittered_clip_from_world",
                        bytemuck::cast_slice(&projection.to_cols_array()),
                    ),
                ],
            );
            let cloud_bytes = uniform_bytes(
                &module,
                1,
                &[
                    (
                        "transform",
                        bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
                    ),
                    ("global_scale", bytemuck::bytes_of(&1.0f32)),
                    ("global_opacity", bytemuck::bytes_of(&1.0f32)),
                    ("count", bytemuck::bytes_of(&1u32)),
                ],
            );
            let shader = raw.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("production Gaussian footprint regression"),
                source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
            });
            let pipeline = raw.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
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
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: default(),
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
            let bind = |group, buffers: &[&wgpu::Buffer]| {
                raw.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &pipeline.get_bind_group_layout(group),
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
            let view = make(&view_bytes, wgpu::BufferUsages::UNIFORM);
            let cloud = make(&cloud_bytes, wgpu::BufferUsages::UNIFORM);
            let view_group = bind(0, &[&view]);
            let uniform_group = bind(1, &[&cloud]);
            // Nonzero directional bands catch position-dependent orthographic
            // shading, not merely equal footprints with constant SH0 colour.
            let mut coefficients = vec![0.0f32; SH_COEFF_COUNT];
            if SH_COEFF_COUNT > 11 {
                coefficients[3] = 0.5;
                coefficients[6] = 0.3;
            }
            let sh = make(
                bytemuck::cast_slice(&coefficients),
                wgpu::BufferUsages::STORAGE,
            );
            let rotation = make(
                bytemuck::cast_slice(&rotation_wxyz),
                wgpu::BufferUsages::STORAGE,
            );
            let scales = make(
                bytemuck::cast_slice(&scale_opacity),
                wgpu::BufferUsages::STORAGE,
            );
            let sorted = make(
                bytemuck::cast_slice(&[0u32, 0]),
                wgpu::BufferUsages::STORAGE,
            );
            let sort_group = bind(3, &[&sorted]);
            let mut images = Vec::new();
            for &(label, position, should_draw) in cases {
                let position = make(bytemuck::cast_slice(&position), wgpu::BufferUsages::STORAGE);
                let attributes = bind(2, &[&position, &sh, &rotation, &scales]);
                let mut encoder = device.create_command_encoder(&default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &target_view,
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
                    pass.set_bind_group(0, &view_group, &[]);
                    pass.set_bind_group(1, &uniform_group, &[]);
                    pass.set_bind_group(2, &attributes, &[]);
                    pass.set_bind_group(3, &sort_group, &[]);
                    pass.draw(0..4, 0..1);
                }
                encoder.copy_texture_to_buffer(
                    target.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &staging,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(64),
                        },
                    },
                    target.size(),
                );
                queue.submit([encoder.finish()]);
                let (send, receive) = std::sync::mpsc::channel();
                staging.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    let _ = send.send(r);
                });
                device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .expect("GPU poll");
                receive.recv().expect("readback callback").expect("GPU map");
                let image = staging.slice(..).get_mapped_range().to_vec();
                let pixels = image.chunks_exact(4).filter(|p| p[3] != 0).count();
                staging.unmap();
                if (pixels > 0) != should_draw {
                    failures.push(format!("{profile:?}, aabb={aabb}, {label}: {pixels} pixels, expected visible={should_draw}"));
                }
                images.push(image);
            }
            eprintln!(
                "{profile:?}, aabb={aabb}, covered pixels={:?}",
                images
                    .iter()
                    .map(|image| image.chunks_exact(4).filter(|p| p[3] > 0).count())
                    .collect::<Vec<_>>()
            );
            results.push(RasterReadback {
                profile,
                aabb,
                images,
            });
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    results
}
