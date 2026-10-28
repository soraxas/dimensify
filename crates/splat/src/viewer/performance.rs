//! Browser replay diagnostics. Installed only for the exact `perf=1` query.
//! Intervals are main-world/encoded-command cadence, never GPU execution time.

#[cfg(target_arch = "wasm32")]
use bevy::{platform::time::Instant, prelude::*};
use std::collections::VecDeque;

const HISTORY: usize = 4096;

#[derive(Default, serde::Serialize)]
struct FrameHistory {
    total: u64,
    first_at_ms: Option<f64>,
    intervals: VecDeque<(u64, f64, f64)>,
    #[serde(skip)]
    previous: Option<f64>,
}

impl FrameHistory {
    fn observe(&mut self, now: f64) {
        self.total += 1;
        self.first_at_ms.get_or_insert(now);
        if let Some(previous) = self.previous.replace(now)
            && now > previous
        {
            if self.intervals.len() == HISTORY {
                self.intervals.pop_front();
            }
            self.intervals.push_back((self.total, now, now - previous));
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use crate::diagnostics::{sog_cache::SogCacheTrace, sog_cpu::SogCpuTimings};
    use bevy::render::{Render, RenderApp, RenderSystems};
    use bevy_gaussian_splatting::diagnostics::{GaussianDiagnosticSample, GaussianRenderStatus};
    use std::sync::{Arc, Mutex};
    use wasm_bindgen::JsValue;

    #[derive(Resource, Clone)]
    struct EncodedFrames(Arc<Mutex<FrameHistory>>, Instant, f64);
    #[derive(Resource)]
    struct Telemetry {
        frames: FrameHistory,
        started: Instant,
        epoch_ms: f64,
        published_ms: f64,
        phase: &'static str,
        phase_changed_at_ms: f64,
    }

    pub(crate) fn install(app: &mut App) {
        let query = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let parameters: std::collections::HashMap<_, _> =
            url::form_urlencoded::parse(query.trim_start_matches('?').as_bytes())
                .into_owned()
                .collect();
        if parameters.get("perf").map(String::as_str) != Some("1") {
            return;
        }
        let setting = |key: &str, max: usize| {
            parameters.get(key).and_then(|value| {
                value
                    .parse::<usize>()
                    .ok()
                    .filter(|value| (1..=max).contains(value))
            })
        };
        if let Some(mut lod) = app
            .world_mut()
            .get_resource_mut::<crate::streaming::SogLodSettings>()
        {
            if let Some(value) = setting("perf_splat_budget", 16_000_000) {
                lod.splat_budget = value;
            }
            if let Some(value) = setting("perf_cache_mib", 2_048) {
                lod.cache_bytes = value * 1024 * 1024;
            }
            if let Some(value) = setting("perf_concurrency", 32) {
                lod.concurrent_chunks = value;
            }
        }
        if let Some(mut budget) = app
            .world_mut()
            .get_resource_mut::<crate::renderer::gpu::GpuBudget>()
            && let Some(value) = setting("perf_source_mib", 16_384)
        {
            budget.source_bytes = (value as u64) * 1024 * 1024;
        }
        let started = Instant::now();
        let epoch_ms = js_sys::Date::now();
        let encoded = EncodedFrames(
            Arc::new(Mutex::new(FrameHistory::default())),
            started,
            epoch_ms,
        );
        app.insert_resource(encoded.clone())
            .insert_resource(Telemetry {
                frames: default(),
                started,
                epoch_ms,
                published_ms: -1000.,
                phase: "empty",
                phase_changed_at_ms: epoch_ms,
            })
            .insert_resource(SogCpuTimings::enabled())
            .insert_resource(SogCacheTrace::enabled())
            .add_systems(Update, tag_camera)
            .add_systems(Last, publish);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(encoded)
                .add_systems(Render, observe_encoded.after(RenderSystems::Render));
        }
    }

    fn tag_camera(
        mut commands: Commands,
        cameras: Query<Entity, With<dimensify_camera::viewer::ViewerCamera>>,
    ) {
        for entity in &cameras {
            commands.entity(entity).insert(GaussianDiagnosticSample(1));
        }
    }

    fn observe_encoded(status: Res<GaussianRenderStatus>, frames: Res<EncodedFrames>) {
        // Acknowledged command encoding is distinct from GPU completion/pixels.
        if status.snapshots().iter().any(|view| view.ready)
            && let Ok(mut history) = frames.0.lock()
        {
            history.observe(frames.2 + frames.1.elapsed().as_secs_f64() * 1000.);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn publish(
        mut telemetry: ResMut<Telemetry>,
        scene: Res<crate::SplatScene>,
        files: Res<super::super::files::LocalFiles>,
        streams: Query<&crate::streaming::runtime::SogStream>,
        cpu: Res<SogCpuTimings>,
        cache: Res<SogCacheTrace>,
        encoded: Res<EncodedFrames>,
        lod: Res<crate::streaming::SogLodSettings>,
        backend: Res<crate::renderer::SogRendererBackend>,
        adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
        budget: Res<crate::renderer::gpu::GpuBudget>,
        cameras: Query<(&Transform, &Projection), With<dimensify_camera::viewer::ViewerCamera>>,
    ) {
        let elapsed = telemetry.started.elapsed().as_secs_f64() * 1000.;
        let now = telemetry.epoch_ms + elapsed;
        telemetry.frames.observe(now);
        let (phase, name, loaded_count) = match &scene.phase {
            crate::LoadPhase::Empty => ("empty", "", 0),
            crate::LoadPhase::Loading(name) => ("loading", name.as_str(), 0),
            crate::LoadPhase::Ready { name, count } => ("ready", name.as_str(), *count),
            crate::LoadPhase::Failed(error) => ("failed", error.as_str(), 0),
        };
        if telemetry.phase != phase {
            telemetry.phase = phase;
            telemetry.phase_changed_at_ms = now;
        }
        if elapsed - telemetry.published_ms < 100. {
            return;
        }
        telemetry.published_ms = elapsed;
        let mut pending_downloads = 0usize;
        let mut pending_generations = 0usize;
        let mut streaming_busy = false;
        let mut displayed = 0usize;
        let mut ready_bytes = 0u64;
        let mut reserved_bytes = 0u64;
        let mut stream_count = 0usize;
        let mut stream_statuses = Vec::new();
        for stream in &streams {
            let status = stream.status();
            stream_count += 1;
            if stream_statuses.len() < 4 {
                stream_statuses.push(serde_json::json!({ "evaluated": status.evaluated, "requested": status.requested, "effective": status.effective, "selected": status.selected, "displayed": status.displayed, "limiting_reason": status.limiting_reason, "pending_downloads": status.pending_downloads, "pending_generation": status.pending_generation, "pending_work": status.pending_work }));
            }
            pending_downloads += status.pending_downloads;
            pending_generations += usize::from(status.pending_generation);
            streaming_busy |= !status.evaluated
                || status.pending_downloads > 0
                || status.pending_generation
                || status.pending_work;
            displayed += status.displayed;
            ready_bytes += status.cache_ready_bytes;
            reserved_bytes += status.cache_reserved_bytes;
        }
        if stream_count == 0 && phase == "ready" {
            displayed = loaded_count;
        }
        let camera = cameras.iter().next().map(|(transform, projection)| {
            let (fov, near, far) = match projection { Projection::Perspective(p) => (Some(p.fov), Some(p.near), Some(p.far)), _ => (None, None, None) };
            serde_json::json!({ "position": transform.translation.to_array(), "rotation_xyzw": transform.rotation.to_array(), "fov_radians": fov, "near": near, "far": far })
        });
        let Ok(render_frames) = encoded.0.lock() else {
            return;
        };
        let snapshot = serde_json::json!({
            "schema": "dimensify-browser-telemetry/1", "observed_at_ms": now,
            "phase": phase, "phase_changed_at_ms": telemetry.phase_changed_at_ms,
            "name": name, "notice": files.notice, "render_ready_splats": displayed,
            "render_ready_count_semantics": "main-world logical ready selection; not pixel contribution",
            "streaming_busy": streaming_busy, "stream_count": stream_count, "stream_statuses": stream_statuses, "pending_downloads": pending_downloads, "pending_generations": pending_generations,
            "app_frames": telemetry.frames, "renderer_frames": *render_frames,
            "renderer_encoded_frames": render_frames.total,
            "renderer_frame_semantics": "render schedule observed sort+draw encoding; not GPU completion or presentation",
            "cache_ready_bytes": ready_bytes, "cache_reserved_bytes": reserved_bytes,
            "adapter": adapter.map(|info| serde_json::json!({ "name": info.name, "backend": format!("{:?}", info.backend), "device_type": format!("{:?}", info.device_type), "driver": info.driver, "driver_info": info.driver_info })),
            "cpu": cpu.report(), "cache": cache.report(), "camera": camera,
            "settings": { "backend": crate::runtime::backend_name(*backend), "lod_policy": format!("{:?}", lod.lod_policy), "target_pixels": lod.target_pixels, "splat_budget": lod.splat_budget, "cpu_cache_bytes": lod.cache_bytes, "concurrent_chunks": lod.concurrent_chunks, "visibility": format!("{:?}", lod.effective_visibility()), "gpu_source_budget_bytes": budget.source_bytes, "gpu_selection_budget_bytes": budget.selected_bytes_per_scene, "compiled_sh_degree": bevy_gaussian_splatting::material::spherical_harmonics::SH_DEGREE, "msaa": "off", "tonemapping": "none" },
            "visible_gpu_survivors": null, "visible_gpu_survivors_unavailable_reason": "logical selection count only; no survivor readback",
            "total_gpu_memory_bytes": null, "total_gpu_memory_unavailable_reason": "only source-cache payload and configured budgets are reported",
            "worker_execution_ms": null, "worker_execution_unavailable_reason": "CPU stage totals are elapsed wall intervals across asynchronous jobs; no worker-handler timer",
            "gpu_frame_time_ms": null, "gpu_timing_unavailable_reason": "viewer replay does not install timestamp query readback",
            "frame_completion_ms": null, "frame_completion_unavailable_reason": "continuous production viewer permits multiple frames in flight; no single-flight completion protocol installed",
            "publish_period_ms": 100, "history_capacity": HISTORY,
        });
        if let Ok(value) = js_sys::JSON::parse(&snapshot.to_string()) {
            let _ = js_sys::Reflect::set(
                &js_sys::global(),
                &JsValue::from_str("__DIMENSIFY_PERF__"),
                &value,
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) use browser::install;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_history_is_bounded_and_preserves_sequence_and_gaps() {
        let mut history = FrameHistory::default();
        for i in 0..HISTORY + 10 {
            history.observe(i as f64 * 2.);
        }
        assert_eq!(history.total, (HISTORY + 10) as u64);
        assert_eq!(history.first_at_ms, Some(0.));
        assert_eq!(history.intervals.len(), HISTORY);
        assert_eq!(history.intervals.front().unwrap().0, 11);
        assert_eq!(history.intervals.back().unwrap().2, 2.);
    }
}
