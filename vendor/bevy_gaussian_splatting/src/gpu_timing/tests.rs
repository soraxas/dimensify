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
