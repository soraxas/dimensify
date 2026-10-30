//! Bounded streaming-quality observations, independent of presets and presentation.
use crate::{
    renderer::{
        SelectionMemory,
        gpu::{GpuBudget, GpuResident},
        limits::SelectionLimits,
    },
    streaming::{
        SogLodPolicy, SogLodSettings,
        runtime::{SogStream, diagnostics::SogLeafPipeline},
    },
};
use bevy::prelude::*;

/// Maximum scene headers retained by the diagnostic snapshot.
pub const MAX_DIAGNOSTIC_SCENES: usize = 4;
/// Maximum leaf records retained across the diagnostic snapshot.
pub const MAX_DIAGNOSTIC_LEAVES: usize = 256;

/// Opt-in bounded leaf evidence; ordinary quality observations remain available.
#[derive(Resource, Default, Clone, Copy)]
pub struct SogDiagnosticSettings {
    pub enabled: bool,
}

/// Synchronization points for consumer configuration and presentation adapters.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SplatDiagnosticsSet {
    Configure,
    Publish,
}

/// Latest bounded domain evidence. No history or synchronous GPU readback.
#[derive(Resource, Default, Clone, PartialEq, serde::Serialize)]
pub struct SogLeafSnapshot {
    pub backend: &'static str,
    pub compiled_sh_degree: u32,
    pub gaussian_cameras: usize,
    /// Full inventory counts, before bounded diagnostic truncation.
    pub scene_count: usize,
    pub leaf_count: usize,
    pub selection_limits: Option<SelectionLimits>,
    pub scenes: Vec<SceneLeafSnapshot>,
}

#[derive(Clone, PartialEq, serde::Serialize)]
pub struct SceneLeafSnapshot {
    pub scene: u64,
    pub requested: usize,
    pub effective: usize,
    pub accepted_selected: usize,
    pub pending_downloads: usize,
    pub pending_generation: bool,
    pub pending_work: bool,
    pub reason: String,
    pub cache_ready_bytes: u64,
    pub cache_reserved_bytes: u64,
    pub gpu_source_allocated_bytes: u64,
    pub selection_memory: SelectionMemory,
    pub selection_gpu_is_estimate: bool,
    pub leaves: Vec<SogLeafPipeline>,
}

/// Current selection and memory observations; no presets, persistence or GPU readback.
#[derive(Resource, Clone, Default, PartialEq)]
pub struct SogQualityReport {
    pub available: bool,
    pub lod_policy: SogLodPolicy,
    pub target_pixels: u32,
    pub requested: usize,
    pub effective: usize,
    pub selected: usize,
    pub displayed: usize,
    pub limit_summary: String,
    pub activity: String,
    pub sh_degree: u32,
    /// UI-ready explanation: compiled float layout or runtime packed-source bands.
    pub sh_description: String,
    pub memory_summary: String,
    pub limiting_reason: String,
    pub leaf_diagnostics: bool,
}

/// Observe existing streaming resources without creating a window or renderer.
#[derive(Default)]
pub struct SplatDiagnosticsPlugin;
impl Plugin for SplatDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SogLodSettings>()
            .init_resource::<GpuBudget>()
            .init_resource::<GpuResident>()
            .init_resource::<crate::renderer::SogRendererBackend>()
            .init_resource::<SogDiagnosticSettings>()
            .init_resource::<SogQualityReport>()
            .init_resource::<SogLeafSnapshot>()
            .add_systems(
                Update,
                configure_streams.in_set(SplatDiagnosticsSet::Configure),
            )
            .add_systems(
                PostUpdate,
                collect
                    .in_set(SplatDiagnosticsSet::Publish)
                    .after(crate::streaming::runtime::update_sog_streams)
                    .after(crate::renderer::gpu::cleanup),
            );
    }
}

fn configure_streams(settings: Res<SogDiagnosticSettings>, mut streams: Query<&mut SogStream>) {
    for mut stream in &mut streams {
        if settings.is_changed() || stream.is_added() {
            stream.configure_diagnostics(settings.enabled);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect(
    diagnostics: Res<SogDiagnosticSettings>,
    lod: Res<SogLodSettings>,
    budget: Res<GpuBudget>,
    streams: Query<(Entity, &SogStream)>,
    cameras: Query<(), With<bevy_gaussian_splatting::GaussianCamera>>,
    backend: Res<crate::renderer::SogRendererBackend>,
    overrides: Query<&crate::runtime::SogStreamRenderSettings>,
    gpu: Res<GpuResident>,
    packed: Option<Res<crate::renderer::packed::PackedResident>>,
    projection: Option<Res<crate::runtime::SogProjectionCache>>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
    mut model: ResMut<SogQualityReport>,
    mut snapshot: ResMut<SogLeafSnapshot>,
) {
    let mut live: Vec<_> = streams.iter().collect();
    live.sort_unstable_by_key(|(entity, _)| entity.to_bits());
    let camera_count = cameras.iter().count();
    let backend_for = |entity| {
        if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
            crate::renderer::SogRendererBackend::Snapshot
        } else {
            overrides.get(entity).map_or(*backend, |s| s.backend)
        }
    };
    let effective_backend = live
        .first()
        .map_or(*backend, |(entity, _)| backend_for(*entity));
    let mixed = live
        .iter()
        .any(|(entity, _)| backend_for(*entity) != effective_backend);
    let is_packed = effective_backend == crate::renderer::SogRendererBackend::Packed;
    let resident = effective_backend != crate::renderer::SogRendererBackend::Snapshot;
    let source_bytes = |entity| match backend_for(entity) {
        crate::renderer::SogRendererBackend::Packed => {
            packed.as_ref().map_or(0, |p| p.scene_source_bytes(entity))
        }
        crate::renderer::SogRendererBackend::GpuResident => gpu.scene_source_bytes(entity),
        crate::renderer::SogRendererBackend::Snapshot => 0,
    };
    let projection_for = |entity| {
        overrides.get(entity).map_or_else(
            |_| projection.as_ref().is_some_and(|p| p.0),
            |s| s.projection_cache,
        )
    };
    let limits_for = |entity| {
        SelectionLimits::from_device(
            device.as_deref(),
            backend_for(entity),
            budget.selected_bytes_per_scene,
            camera_count,
        )
        .with_projection_cache(
            backend_for(entity) == crate::renderer::SogRendererBackend::Packed
                && projection_for(entity),
        )
    };
    let limits = limits_for(live.first().map_or(Entity::PLACEHOLDER, |(e, _)| *e));
    let selection_memory = |entity, stream: &SogStream| match backend_for(entity) {
        crate::renderer::SogRendererBackend::Packed => packed
            .as_ref()
            .map_or(SelectionMemory::default(), |p| p.scene_memory(entity)),
        crate::renderer::SogRendererBackend::GpuResident => gpu.scene_memory(entity),
        crate::renderer::SogRendererBackend::Snapshot => stream.snapshot_memory(camera_count),
    };
    let mut next_snapshot = SogLeafSnapshot::default();
    let mut remaining = MAX_DIAGNOSTIC_LEAVES;
    if diagnostics.enabled {
        next_snapshot.selection_limits = (!mixed).then_some(limits);
        next_snapshot.scene_count = live.len();
        next_snapshot.leaf_count = live.iter().map(|(_, stream)| stream.leaf_count()).sum();
        next_snapshot.backend = if mixed {
            "mixed"
        } else {
            crate::runtime::backend_name(effective_backend)
        };
        next_snapshot.compiled_sh_degree =
            bevy_gaussian_splatting::material::spherical_harmonics::SH_DEGREE as u32;
        next_snapshot.gaussian_cameras = cameras.iter().count();
        for (entity, stream) in live.iter().take(MAX_DIAGNOSTIC_SCENES) {
            let status = stream.status();
            let leaves = stream.leaf_snapshot(remaining);
            remaining -= leaves.len();
            next_snapshot.scenes.push(SceneLeafSnapshot {
                scene: entity.to_bits(),
                requested: status.requested,
                effective: status.effective,
                accepted_selected: status.selected,
                pending_downloads: status.pending_downloads,
                pending_generation: status.pending_generation,
                pending_work: status.pending_work,
                reason: status.limiting_reason,
                cache_ready_bytes: status.cache_ready_bytes,
                cache_reserved_bytes: status.cache_reserved_bytes,
                gpu_source_allocated_bytes: source_bytes(*entity),
                selection_memory: selection_memory(*entity, stream),
                selection_gpu_is_estimate: backend_for(*entity)
                    == crate::renderer::SogRendererBackend::Snapshot,
                leaves,
            });
        }
    }
    if *snapshot != next_snapshot {
        // At most 4 scene headers / 256 leaves in a log, only on evidence change.
        if diagnostics.enabled {
            trace!(
                "SOG leaf diagnostics {}",
                serde_json::to_string(&next_snapshot).unwrap_or_default()
            );
        }
        *snapshot = next_snapshot;
    }
    let statuses: Vec<_> = live.iter().map(|(_, stream)| stream.status()).collect();
    let effective = statuses
        .iter()
        .map(|status| status.effective)
        .min()
        .unwrap_or(0);
    let selected = statuses
        .iter()
        .fold(0usize, |sum, status| sum.saturating_add(status.selected));
    let displayed = statuses
        .iter()
        .fold(0usize, |sum, status| sum.saturating_add(status.displayed));
    let memory_limited = statuses
        .iter()
        .any(|s| s.source_limited || s.effective < lod.splat_budget);
    let selection_limited = statuses.iter().any(|s| s.selection_limited);
    let limit_summary = if limits.webgl_texture_dimension.is_some() && memory_limited {
        limits.request_constraint(
            lod.splat_budget,
            effective,
            statuses.iter().any(|s| s.source_limited),
        )
    } else {
        match (memory_limited, selection_limited) {
            (true, true) => "Memory + budget",
            (true, false) => "Memory / source",
            (false, true) => "Splat budget",
            (false, false) => "None reported",
        }
    }
    .into();
    let activity = if statuses.iter().any(|s| !s.evaluated) {
        "Evaluating"
    } else if statuses.iter().any(|s| s.pending_generation) {
        "Uploading"
    } else if statuses
        .iter()
        .any(|s| s.pending_work || s.pending_downloads > 0)
    {
        "Refining"
    } else if memory_limited || selection_limited {
        "Capped"
    } else {
        "Settled"
    }
    .into();
    let mut reasons: Vec<_> = statuses
        .iter()
        .map(|status| status.limiting_reason.as_str())
        .collect();
    reasons.sort_unstable();
    reasons.dedup();
    let mut reason = reasons.into_iter().take(4).collect::<Vec<_>>().join("\n");
    if !live.is_empty() && effective < lod.splat_budget {
        reason.insert_str(
            0,
            &format!(
                "Request limited by {}.\n",
                limits.request_constraint(
                    lod.splat_budget,
                    effective,
                    statuses.iter().any(|s| s.source_limited)
                )
            ),
        );
    }
    if diagnostics.enabled {
        reason.push_str(&format!(
            "\nLeaf snapshot: {} / 256 records; {} / {} scenes (max 4).",
            MAX_DIAGNOSTIC_LEAVES - remaining,
            snapshot.scenes.len(),
            live.len()
        ));
    }
    let ready = statuses
        .iter()
        .fold(0u64, |n, status| n.saturating_add(status.cache_ready_bytes));
    let reserved = statuses.iter().fold(0u64, |n, status| {
        n.saturating_add(status.cache_reserved_bytes)
    });
    let total_selection =
        live.iter()
            .fold(SelectionMemory::default(), |mut total, (entity, stream)| {
                let memory = selection_memory(*entity, stream);
                total.selected_gpu_bytes = total
                    .selected_gpu_bytes
                    .saturating_add(memory.selected_gpu_bytes);
                total.radix_reserved_bytes = total
                    .radix_reserved_bytes
                    .saturating_add(memory.radix_reserved_bytes);
                total.cpu_snapshot_bytes = total
                    .cpu_snapshot_bytes
                    .saturating_add(memory.cpu_snapshot_bytes);
                total
            });
    let mut memory_summary = format!(
        "CPU decoded cache (all scenes): {} B ready + {} B in-flight reserved; ceiling {} MiB/scene\nGPU immutable sources (global, resident mode): {} B allocated / {} MiB ceiling",
        ready,
        reserved,
        lod.cache_bytes / (1024 * 1024),
        gpu.statistics
            .resident_bytes
            .saturating_add(packed.as_ref().map_or(0, |p| p.statistics.resident_bytes)),
        budget.source_bytes / (1024 * 1024),
    );
    memory_summary.insert_str(0, &format!("{}\n", limits.explanation()));
    memory_summary.push_str(&format!(
        "\n{} (all scenes): {} B + radix {} B reserved; ceiling {} MiB/scene\nCPU snapshot planes (all scenes): {} B allocated",
        if mixed { "Selection allocation/estimate (per-scene scopes below)" } else if resident { "Persistent slots allocated" } else { "Snapshot GPU estimate" },
        total_selection.selected_gpu_bytes, total_selection.radix_reserved_bytes,
        budget.selected_bytes_per_scene / (1024 * 1024), total_selection.cpu_snapshot_bytes,
    ));
    for (entity, stream) in live.iter().take(MAX_DIAGNOSTIC_SCENES) {
        let resident = backend_for(*entity) != crate::renderer::SogRendererBackend::Snapshot;
        let status = stream.status();
        let memory = selection_memory(*entity, stream);
        memory_summary.push_str(&format!(
            "\nScene {}: cache {} B ready + {} B reserved / {} MiB ceiling; resident source {} B allocated (global ceiling above)\n  {} {} B + radix {} B reserved / {} MiB selection ceiling; {} {}\n  CPU snapshot planes: {} B allocated",
            entity.to_bits(), status.cache_ready_bytes, status.cache_reserved_bytes,
            lod.cache_bytes / (1024 * 1024), source_bytes(*entity),
            if resident { "Persistent slots allocated:" } else { "Snapshot GPU estimate:" },
            memory.selected_gpu_bytes, memory.radix_reserved_bytes,
            budget.selected_bytes_per_scene / (1024 * 1024), memory.generations,
            if resident { "high-water slots" } else { "old/pending generations" },
            memory.cpu_snapshot_bytes,
        ));
    }
    if live.len() > MAX_DIAGNOSTIC_SCENES {
        memory_summary.push_str(&format!(
            "\nPer-scene details: first {} / {} scenes.",
            MAX_DIAGNOSTIC_SCENES,
            live.len()
        ));
    }
    memory_summary.push_str(&format!(
        "\nGaussian cameras: {camera_count}. {}\nTemporary decode, upstream CPU sort arrays and asynchronous retired buffers additional; no GPU readback.",
        if resident { "Allocated slots remain reusable after handoff; radix is a conservative reservation." }
        else if limits.webgl_texture_dimension.is_some() { "GPU estimates include padded attribute textures and per-view indices; CPU worker scratch and transfer copies are additional." }
        else { "GPU estimates include square-padded sort entries per Gaussian camera; radix is a conservative reservation, not measured allocation." },
    ));
    let next = SogQualityReport {
        available: !live.is_empty(),
        lod_policy: lod.lod_policy,
        target_pixels: lod.target_pixels,
        requested: lod.splat_budget,
        effective,
        selected,
        displayed,
        limit_summary,
        activity,
        sh_degree: bevy_gaussian_splatting::material::spherical_harmonics::SH_DEGREE as u32,
        sh_description: if mixed {
            "Mixed stream backends during replacement; SH layout and selection allocation scope differ by scene.".into()
        } else if is_packed {
            let description = "SH up to 3 (packed source bands, evaluated per view).";
            #[cfg(target_arch = "wasm32")]
            {
                format!(
                    "{description}\n{}",
                    crate::streaming::runtime::codec::description()
                )
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                format!("{description}\nCodec: serial.")
            }
        } else {
            format!(
                "SH{} (compiled).",
                bevy_gaussian_splatting::material::spherical_harmonics::SH_DEGREE
            )
        },
        memory_summary,
        limiting_reason: reason,
        leaf_diagnostics: diagnostics.enabled,
    };
    if *model != next {
        *model = next;
    }
}
