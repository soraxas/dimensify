//! Opt-in bounded GPU timing; application/provider policy stays with the caller.
mod state;

use crate::timestamps::GaussianTimestampInterval;
use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_component::ExtractComponentPlugin,
        renderer::{RenderAdapterInfo, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue},
    },
};
pub use state::{Capability, Failure, Identity, Measurement, Report, Sample};
use state::{Phase, SLOT_COUNT, State};
use std::sync::{Arc, Mutex};

/// Consumer-supplied opaque IDs. Replacing a selection must replace its tag.
#[derive(Component, Clone, Copy, bevy::render::extract_component::ExtractComponent)]
pub struct GaussianTimingSource {
    pub scene: u64,
    pub selection: u64,
}

/// Only explicitly tagged Gaussian cameras are sampled (never fallback UI cameras).
#[derive(Component, Clone, Copy, bevy::render::extract_component::ExtractComponent)]
pub struct GaussianTimingView;

#[derive(Resource, Clone, Default)]
pub struct GaussianGpuTimings(Arc<Mutex<State>>);

impl GaussianGpuTimings {
    /// Bounded historical results; these retain originating IDs, not current scene IDs.
    pub fn report(&self) -> Option<Report> {
        self.0.lock().ok().map(|state| state.report.clone())
    }
}

#[derive(Resource, Default)]
pub(crate) struct Recorder {
    initialized: bool,
    frame: u64,
    slots: Vec<Slot>,
    period_ns: f32,
    metal: bool,
}

struct Slot {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
}

/// Explicit consumer opt-in. This never requests device features or enables
/// Bevy's CPU/tracing recorder. An unsupported actual device stays unsupported.
pub struct GaussianGpuTimingPlugin;

impl Plugin for GaussianGpuTimingPlugin {
    fn build(&self, app: &mut App) {
        let bridge = GaussianGpuTimings::default();
        app.insert_resource(bridge.clone()).add_plugins((
            ExtractComponentPlugin::<GaussianTimingSource>::default(),
            ExtractComponentPlugin::<GaussianTimingView>::default(),
        ));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(bridge)
                .init_resource::<Recorder>()
                .add_systems(Render, prepare.in_set(RenderSystems::PrepareViews))
                .add_systems(RenderGraph, submitted.in_set(RenderGraphSystems::Finish));
        }
    }
}

impl Recorder {
    pub(crate) fn record(
        &self,
        bridge: &GaussianGpuTimings,
        view: Entity,
        source: GaussianTimingSource,
        count: usize,
        compact: bool,
    ) -> Option<GaussianTimestampInterval> {
        let ticket = bridge.0.lock().ok()?.reserve(Identity {
            frame: self.frame,
            view: view.to_bits(),
            scene: source.scene,
            selection: source.selection,
            input_rows: count,
            compaction: compact,
        })?;
        let slot = self.slots.get(ticket.slot)?;
        Some(GaussianTimestampInterval {
            query_set: slot.queries.clone(),
            begin: 0,
            end: 1,
            intermediate: Some(2..5),
        })
    }
}

fn prepare(
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    adapter: Res<RenderAdapterInfo>,
    bridge: Res<GaussianGpuTimings>,
    mut recorder: ResMut<Recorder>,
) {
    if !recorder.initialized {
        recorder.initialized = true;
        recorder.period_ns = queue.get_timestamp_period();
        recorder.metal = adapter.backend == wgpu::Backend::Metal;
        let supported = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let valid_period = recorder.period_ns.is_finite() && recorder.period_ns > 0.0;
        if let Ok(mut state) = bridge.0.lock() {
            state.report.capability = if !supported {
                Capability::UnsupportedTimestampQuery
            } else if !valid_period {
                Capability::InvalidTimestampPeriod
            } else {
                Capability::TimestampQuery
            };
        }
        if supported && valid_period {
            for _ in 0..SLOT_COUNT {
                recorder.slots.push(Slot {
                    queries: device
                        .wgpu_device()
                        .create_query_set(&wgpu::QuerySetDescriptor {
                            label: Some("Gaussian radix timestamps"),
                            ty: wgpu::QueryType::Timestamp,
                            // Four radix digits at most: initial/final plus
                            // three intervening writes. Keep every pass timed.
                            count: 5,
                        }),
                    resolve: device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Gaussian radix resolve"),
                        size: 16,
                        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                        mapped_at_creation: false,
                    }),
                    readback: device.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Gaussian radix readback"),
                        size: 16,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                });
            }
        }
    }
    let Some(frame) = recorder.frame.checked_add(1) else {
        return;
    };
    recorder.frame = frame;
    // A nonblocking poll on native; browser callbacks are driven by its event loop.
    if device.poll(wgpu::PollType::Poll).is_err() {
        if let Ok(mut state) = bridge.0.lock() {
            state.stop();
        }
        return;
    }
    let tickets = bridge
        .0
        .lock()
        .ok()
        .map(|s| s.tickets_in(Phase::Ready))
        .unwrap_or_default();
    for ticket in tickets {
        let Some(slot) = recorder.slots.get(ticket.slot) else {
            continue;
        };
        if !bridge.0.lock().is_ok_and(|mut state| state.mapping(ticket)) {
            continue;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Gaussian timestamp readback"),
        });
        encoder.resolve_query_set(&slot.queries, 0..2, &slot.resolve, 0);
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, 16);
        queue.submit([encoder.finish()]);
        let readback = slot.readback.clone();
        let bridge = bridge.clone();
        let period = recorder.period_ns;
        let metal = recorder.metal;
        slot.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let measurement = if result.is_ok() {
                    let measurement = {
                        let range = readback.slice(..).get_mapped_range();
                        match range.as_chunks::<8>().0 {
                            [begin, end] => Measurement::from_ticks(
                                [u64::from_le_bytes(*begin), u64::from_le_bytes(*end)],
                                period,
                                metal,
                            ),
                            _ => Measurement::failed(Failure::InvalidTimestamp),
                        }
                    };
                    readback.unmap();
                    measurement
                } else {
                    Measurement::failed(Failure::MapFailed)
                };
                if let Ok(mut state) = bridge.0.lock() {
                    state.complete(ticket, measurement);
                }
            });
    }
}

fn submitted(queue: Res<RenderQueue>, bridge: Res<GaussianGpuTimings>) {
    let tickets = bridge
        .0
        .lock()
        .ok()
        .map(|s| s.tickets_in(Phase::Recording))
        .unwrap_or_default();
    for ticket in tickets {
        if !bridge
            .0
            .lock()
            .is_ok_and(|mut state| state.submitted(ticket))
        {
            continue;
        }
        let bridge = bridge.clone();
        // Completion establishes readiness only, never a duration. Resolve on
        // a later prepare tick; keep the lease until map callback + unmap.
        queue.on_submitted_work_done(move || {
            if let Ok(mut state) = bridge.0.lock() {
                state.ready(ticket);
            }
        });
    }
}

#[cfg(test)]
pub(crate) mod tests;
