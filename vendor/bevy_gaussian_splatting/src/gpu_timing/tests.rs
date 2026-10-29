use super::*;
use bevy::{
    ecs::system::RunSystemOnce,
    render::settings::{Backends, WgpuFeatures, WgpuSettings},
};

#[test]
#[ignore = "offscreen native device capability check"]
fn device_without_timestamp_query_stays_unavailable_without_allocating_slots() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings {
            disabled_features: Some(
                WgpuFeatures::TIMESTAMP_QUERY
                    | WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS
                    | WgpuFeatures::TIMESTAMP_QUERY_INSIDE_PASSES
                    | WgpuFeatures::PIPELINE_STATISTICS_QUERY,
            ),
            ..default()
        },
    ));
    let mut app = App::new();
    app.insert_resource(resources.0)
        .insert_resource(resources.1)
        .insert_resource(resources.2)
        .init_resource::<GaussianGpuTimings>()
        .init_resource::<Recorder>()
        .add_systems(Update, prepare);
    app.update();
    let bridge = app.world().resource::<GaussianGpuTimings>();
    let recorder = app.world().resource::<Recorder>();
    assert!(recorder.slots.is_empty());
    assert!(recorder.pass_slots.iter().all(Vec::is_empty));
    for pass in bridge.pass_reports().expect("pass capability reports") {
        assert_eq!(pass.capability, Capability::UnsupportedTimestampQuery);
        assert!(pass.samples.is_empty());
        assert_eq!(pass.pending, 0);
    }
    let report = bridge.report().expect("capability report");
    assert_eq!(report.capability, Capability::UnsupportedTimestampQuery);
    assert!(report.samples.is_empty());
    assert_eq!(report.pending, 0);
}

/// Test-only driver around the actual collector, shared by production radix tests.
pub(crate) struct Probe {
    app: App,
    bridge: GaussianGpuTimings,
    view: Entity,
    completed: u64,
}

impl Probe {
    pub(crate) fn new(
        device: RenderDevice,
        queue: RenderQueue,
        adapter: RenderAdapterInfo,
    ) -> Self {
        let mut app = App::new();
        app.insert_resource(device)
            .insert_resource(queue)
            .insert_resource(adapter)
            .init_resource::<GaussianGpuTimings>()
            .init_resource::<Recorder>()
            .add_systems(Update, prepare);
        app.update();
        let view = app.world_mut().spawn_empty().id();
        let bridge = app.world().resource::<GaussianGpuTimings>().clone();
        assert_eq!(
            bridge.report().expect("report").capability,
            Capability::TimestampQuery
        );
        Self {
            app,
            bridge,
            view,
            completed: 0,
        }
    }

    pub(crate) fn record(&self, count: usize, compact: bool) -> GaussianTimestampInterval {
        self.app
            .world()
            .resource::<Recorder>()
            .record(
                &self.bridge,
                self.view,
                GaussianTimingSource {
                    scene: 42,
                    selection: self.completed,
                },
                count,
                compact,
            )
            .expect("prior probe sample drained")
    }

    pub(crate) fn finish(&mut self, count: usize, compact: bool) {
        self.app
            .world_mut()
            .run_system_once(submitted)
            .expect("submission hook");
        let start = std::time::Instant::now();
        while self.bridge.report().expect("report").pending != 0 {
            assert!(
                start.elapsed().as_secs() < 10,
                "GPU timestamp readback deadline"
            );
            self.app.update();
            std::thread::yield_now();
        }
        let report = self.bridge.report().expect("report");
        let sample = report.samples.back().expect("completed sample");
        assert_eq!(
            sample.identity,
            Identity {
                frame: sample.identity.frame,
                view: self.view.to_bits(),
                scene: 42,
                selection: self.completed,
                input_rows: count,
                compaction: compact
            }
        );
        assert!(
            sample.measurement.duration_ms.is_some(),
            "invalid production timestamp: {:?}",
            sample.measurement
        );
        self.completed += 1;
        assert_eq!(report.completed, self.completed);
        assert_eq!(report.invalid, 0);
        assert_eq!(report.dropped_busy, 0);
        assert!(report.samples.len() <= report.history_capacity);
    }
}

#[test]
#[ignore = "requires timestamp-capable native GPU; offscreen correctness only"]
fn bounded_recorder_reads_actual_pass_timestamps_without_waiting_on_rendering() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings {
            disabled_features: Some(
                WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS
                    | WgpuFeatures::TIMESTAMP_QUERY_INSIDE_PASSES
                    | WgpuFeatures::PIPELINE_STATISTICS_QUERY,
            ),
            ..default()
        },
    ));
    eprintln!("GPU timestamp correctness adapter={:?}", resources.2.0);
    let device = resources.0.clone();
    let queue = resources.1.clone();
    assert!(
        device.features().contains(wgpu::Features::TIMESTAMP_QUERY),
        "explicit probe needs timestamps"
    );
    let shader = device.wgpu_device().create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("timing probe work"), source: wgpu::ShaderSource::Wgsl(
            "@group(0) @binding(0) var<storage, read_write> value: u32; @compute @workgroup_size(1) fn work() { for (var i=0u; i<1024u; i++) { value = value * 1664525u + 1013904223u; } }".into()) });
    let pipeline = device
        .wgpu_device()
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("timing probe pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("work"),
            compilation_options: default(),
            cache: None,
        });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("timing probe storage"),
        size: 4,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let group = device
        .wgpu_device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("timing probe group"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
    let mut app = App::new();
    app.insert_resource(resources.0)
        .insert_resource(resources.1)
        .insert_resource(resources.2)
        .init_resource::<GaussianGpuTimings>()
        .init_resource::<Recorder>()
        .add_systems(Update, prepare);
    app.update();
    let view = app.world_mut().spawn_empty().id();
    let bridge = app.world().resource::<GaussianGpuTimings>().clone();
    assert_eq!(
        bridge.report().expect("report").capability,
        Capability::TimestampQuery
    );
    for selection in 0..12 {
        let hook = app
            .world()
            .resource::<Recorder>()
            .record(
                &bridge,
                view,
                GaussianTimingSource {
                    scene: 7,
                    selection,
                },
                32,
                false,
            )
            .expect("free timer slot");
        let mut encoder = device.create_command_encoder(&default());
        for digit in [None, Some(0), Some(1), Some(2), Some(3)] {
            assert!(
                hook.writes(digit, 4).is_some(),
                "untimed intermediate passes lose browser end timestamps: {digit:?}"
            );
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("actual pass boundary"),
                timestamp_writes: hook.writes(digit, 4),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        queue.submit([encoder.finish()]);
        app.world_mut()
            .run_system_once(submitted)
            .expect("submit callback");
        let start = std::time::Instant::now();
        while bridge.report().expect("report").pending != 0 {
            assert!(
                start.elapsed().as_secs() < 10,
                "asynchronous readback deadline"
            );
            app.update();
            std::thread::yield_now();
        }
    }
    // Exercise the same single-pass descriptors used by raster, transfer and
    // projection hooks, independently of radix's multi-pass intermediate guards.
    let target = device
        .wgpu_device()
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("timestamp render target"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
    let target = target.create_view(&default());
    let raster_shader = device.wgpu_device().create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("timestamp raster probe"),
        source: wgpu::ShaderSource::Wgsl(
            "@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> { let p = array<vec2<f32>, 3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0)); return vec4(p[i],0.0,1.0); } @fragment fn fragment() -> @location(0) vec4<f32> { return vec4(1.0,0.5,0.0,1.0); }".into(),
        ),
    });
    let raster_pipeline =
        device
            .wgpu_device()
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("timestamp raster probe"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &raster_shader,
                    entry_point: Some("vertex"),
                    buffers: &[],
                    compilation_options: default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &raster_shader,
                    entry_point: Some("fragment"),
                    compilation_options: default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
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
    for scope in PassScope::ALL {
        let hook = app
            .world()
            .resource::<Recorder>()
            .record_pass(
                &bridge,
                scope,
                view,
                GaussianTimingSource {
                    scene: 7,
                    selection: 99,
                },
                32,
                false,
            )
            .expect("independent pass slot");
        let mut encoder = device.create_command_encoder(&default());
        if scope == PassScope::Projection {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("projection descriptor probe"),
                timestamp_writes: Some(hook.compute_writes()),
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("render descriptor probe"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: Some(hook.render_writes()),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&raster_pipeline);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        app.world_mut()
            .run_system_once(submitted)
            .expect("pass submit hook");
        let start = std::time::Instant::now();
        while bridge
            .pass_reports()
            .expect("pass reports")
            .iter()
            .any(|r| r.pending != 0)
        {
            assert!(
                start.elapsed().as_secs() < 10,
                "pass timestamp readback deadline"
            );
            app.update();
            std::thread::yield_now();
        }
        let reports = bridge.pass_reports().expect("pass reports");
        let report = &reports[scope.index()];
        assert_eq!(report.scope, scope.label());
        assert_eq!(report.completed, 1);
        assert_eq!(report.invalid, 0, "{scope:?}: {:?}", report.samples);
        let sample = &report.samples[0];
        assert_eq!(sample.identity.selection, 99);
        assert_eq!(sample.identity.scene, 7);
        assert_eq!(sample.identity.view, view.to_bits());
        assert!(sample.measurement.duration_ms.is_some());
    }
    let report = bridge.report().expect("report");
    eprintln!(
        "GPU timestamp correctness report={}",
        serde_json::to_string(&report).expect("json")
    );
    assert_eq!(report.completed, 12);
    assert_eq!(
        report.invalid, 0,
        "actual device must demonstrate valid writes"
    );
    assert_eq!(report.slot_capacity, SLOT_COUNT);
    assert_eq!(report.pending, 0);
    for (index, sample) in report.samples.iter().enumerate() {
        assert_eq!(sample.identity.selection, index as u64);
        assert_eq!(sample.identity.scene, 7);
        assert_eq!(sample.identity.view, view.to_bits());
        assert!(
            sample
                .measurement
                .duration_ms
                .is_some_and(|value| value >= 0.0)
        );
    }
}
