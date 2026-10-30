//! Snapshot and persistent GPU adapters to the pinned upstream renderer.
//!
//! Owns upstream asset creation, GPU readiness and snapshot lifetime. Transport,
//! LOD policy and CPU cache admission stay with the caller. GPU layout knowledge
//! is isolated in this module; callers submit immutable decoded source ranges.

pub mod accounting;
mod bounds;
pub mod compose;
pub mod gpu;
pub mod limits;
pub use gpu::SogRendererBackend;
pub mod packed;
pub mod readiness;
pub mod source;
#[cfg(feature = "sog-gpu-timing")]
pub mod telemetry;

use crate::diagnostics::sog_cpu::{CpuStage, SogCpuTimings};
use bevy::{ecs::system::SystemParam, prelude::*, render::renderer::RenderDevice};
use bevy_gaussian_splatting::{
    CloudSettings, Gaussian3d, GaussianCamera, GaussianMode, GaussianRasterProfile,
    PlanarGaussian3d, PlanarGaussian3dHandle,
};

pub use crate::streaming::SplatRange;

pub struct DecodedChunk {
    pub cloud: source::SourceData,
    /// Logical count, excluding decoder padding.
    pub count: usize,
}

impl DecodedChunk {
    /// Return the expanded row layout size for the compiled SH degree.
    pub fn bytes_per_splat() -> usize {
        std::mem::size_of::<Gaussian3d>()
    }

    /// Return the allocated row count including renderer padding.
    pub fn padded_rows(&self) -> usize {
        match &self.cloud {
            source::SourceData::Expanded(cloud) => cloud_rows(cloud).max(self.count),
            source::SourceData::Packed(_)
            | source::SourceData::PackedPage { .. }
            | source::SourceData::ResidentPacked(_) => self.count,
        }
    }
}

fn cloud_rows(cloud: &PlanarGaussian3d) -> usize {
    [
        cloud.position_visibility.len(),
        cloud.spherical_harmonic.len(),
        cloud.rotation.len(),
        cloud.scale_opacity.len(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// Live adapter ownership, not GPU completion or total-process memory.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct SelectionMemory {
    pub cpu_snapshot_bytes: u64,
    pub selected_gpu_bytes: u64,
    pub radix_reserved_bytes: u64,
    pub generations: usize,
}

#[derive(Clone, Copy, Default)]
struct SnapshotAllocation {
    rows: usize,
    cpu_bytes: u64,
}

#[derive(Default)]
pub struct SnapshotState {
    current: Option<Entity>,
    pending: Option<Entity>,
    owner: Option<(SogRendererBackend, Entity)>,
    current_allocation: SnapshotAllocation,
    pending_allocation: SnapshotAllocation,
}

impl SnapshotState {
    /// Snapshot GPU values are estimates until upstream asynchronous preparation;
    /// persistent allocations are instead reported by their owning adapter.
    pub fn memory(&self, cameras: usize) -> SelectionMemory {
        if self
            .owner
            .is_some_and(|(backend, _)| backend != SogRendererBackend::Snapshot)
        {
            return SelectionMemory::default();
        }
        let mut memory = SelectionMemory::default();
        for (entity, allocation) in [
            (self.current, self.current_allocation),
            (self.pending, self.pending_allocation),
        ] {
            if entity.is_some() {
                memory.cpu_snapshot_bytes = memory
                    .cpu_snapshot_bytes
                    .saturating_add(allocation.cpu_bytes);
                if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
                    memory.selected_gpu_bytes = memory.selected_gpu_bytes.saturating_add(
                        bevy_gaussian_splatting::render::webgl::selected_gpu_bytes(
                            allocation.rows,
                            cameras,
                        ),
                    );
                } else {
                    memory.selected_gpu_bytes = memory
                        .selected_gpu_bytes
                        .saturating_add(gpu::snapshot_selected_bytes(allocation.rows, cameras));
                    memory.radix_reserved_bytes = memory
                        .radix_reserved_bytes
                        .saturating_add(gpu::scratch_bytes(allocation.rows));
                }
                memory.generations += 1;
            }
        }
        memory
    }
    /// Report whether a render-ready generation is displayed.
    pub fn is_visible(&self) -> bool {
        self.current.is_some()
    }
    /// Report whether a replacement generation awaits readiness.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// Preparation owns reserved sources but has not published a draw generation.
#[derive(Debug, PartialEq, Eq)]
pub enum Submission {
    Preparing,
    Submitted,
}

#[derive(SystemParam)]
pub struct SnapshotRenderer<'w, 's> {
    commands: Commands<'w, 's>,
    clouds: ResMut<'w, Assets<PlanarGaussian3d>>,
    ready: Res<'w, readiness::ReadySnapshots>,
    device: Option<Res<'w, RenderDevice>>,
    backend: Res<'w, SogRendererBackend>,
    scene_settings: Query<'w, 's, &'static super::runtime::SogStreamRenderSettings>,
    gpu: ResMut<'w, gpu::GpuResident>,
    packed: ResMut<'w, packed::PackedResident>,
    queue: Option<Res<'w, bevy::render::renderer::RenderQueue>>,
    render_entities: Query<'w, 's, &'static bevy::render::sync_world::RenderEntity>,
    budget: Res<'w, gpu::GpuBudget>,
    transfers: Res<'w, gpu::GpuTransfers>,
    sorted: Res<'w, Assets<bevy_gaussian_splatting::sort::SortedEntries>>,
    runtime: Res<'w, super::runtime::SogRuntimeSettings>,
    projection: Option<Res<'w, super::runtime::SogProjectionCache>>,
    pages: Option<Res<'w, super::runtime::SogPagedSources>>,
    selections: ResMut<'w, super::runtime::SogSelectionStatistics>,
    cameras: Query<'w, 's, (Entity, &'static Camera, Has<GaussianCamera>)>,
    gaussian_cameras: Query<'w, 's, (), With<GaussianCamera>>,
    cpu: Option<Res<'w, SogCpuTimings>>,
    cache_trace: Option<Res<'w, crate::diagnostics::sog_cache::SogCacheTrace>>,
    frame: Option<Res<'w, bevy::diagnostic::FrameCount>>,
}

impl SnapshotRenderer<'_, '_> {
    fn backend_for(&self, scene: Entity) -> SogRendererBackend {
        if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
            return SogRendererBackend::Snapshot;
        }
        self.scene_settings
            .get(scene)
            .map_or(*self.backend, |s| s.backend)
    }
    fn compaction_for(&self, scene: Entity) -> bool {
        if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
            return false;
        }
        self.scene_settings
            .get(scene)
            .map_or(self.runtime.compaction, |s| s.compaction)
    }
    fn projection_for(&self, scene: Entity) -> bool {
        if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
            return false;
        }
        self.scene_settings.get(scene).map_or_else(
            |_| self.projection.as_ref().is_some_and(|p| p.0),
            |s| s.projection_cache,
        )
    }
    fn budget_for(&self, scene: Entity) -> gpu::GpuBudget {
        let other_sources = match self.backend_for(scene) {
            SogRendererBackend::Packed => self.gpu.statistics.resident_bytes,
            SogRendererBackend::GpuResident => self.packed.statistics.resident_bytes,
            SogRendererBackend::Snapshot => 0,
        };
        gpu::GpuBudget {
            source_bytes: self.budget.source_bytes.saturating_sub(other_sources),
            selected_bytes_per_scene: self.budget.selected_bytes_per_scene,
        }
    }
    /// Retain sources owned by a displayed or pending generation.
    pub fn pin_sources(&mut self, scene: Entity, files: impl IntoIterator<Item = usize>) {
        if self.backend_for(scene) == SogRendererBackend::Packed {
            self.packed.pin_sources(scene, files);
        }
    }
    /// Refresh the whole-file selector's view of GPU residency. Page staging
    /// has separate palette ownership and deliberately does not use this path.
    pub fn sync_source(
        &self,
        scene: Entity,
        file: usize,
        ready: &mut Option<DecodedChunk>,
    ) -> bool {
        self.backend_for(scene) == SogRendererBackend::Packed
            && self.packed.sync_source(scene, file, ready)
    }
    /// Return the optional accounting trace for this scene.
    pub fn cache_trace(&self, scene: Entity) -> crate::diagnostics::sog_cache::SceneTrace {
        self.cache_trace
            .as_ref()
            .map(|trace| trace.scene(scene))
            .unwrap_or_default()
    }
    /// Report whether source residency uses packed pages.
    pub fn uses_pages(&self) -> bool {
        self.uses_pages_for(Entity::PLACEHOLDER)
    }
    pub fn uses_pages_for(&self, scene: Entity) -> bool {
        self.backend_for(scene) == SogRendererBackend::Packed
            && self.pages.as_ref().is_some_and(|p| p.0)
    }
    /// Return the selected immutable source representation.
    pub fn source_storage(&self) -> source::SourceStorage {
        self.source_storage_for(Entity::PLACEHOLDER)
    }
    pub fn source_storage_for(&self, scene: Entity) -> source::SourceStorage {
        if self.backend_for(scene) == SogRendererBackend::Packed {
            source::SourceStorage::Packed
        } else {
            source::SourceStorage::Expanded
        }
    }
    /// Clone the optional bounded CPU timing collector.
    pub fn cpu_timings(&self) -> SogCpuTimings {
        self.cpu.as_deref().cloned().unwrap_or_default()
    }
    /// Return the current selected-row ceiling.
    pub fn splat_limit(&self) -> usize {
        self.splat_limit_for(Entity::PLACEHOLDER)
    }
    pub fn splat_limit_for(&self, scene: Entity) -> usize {
        limits::SelectionLimits::from_device(
            self.device.as_deref(),
            self.backend_for(scene),
            self.budget.selected_bytes_per_scene,
            self.gaussian_cameras.iter().count(),
        )
        .with_projection_cache(
            self.projection_for(scene) && self.backend_for(scene) == SogRendererBackend::Packed,
        )
        .effective_splats
    }

    /// Scene's union capacity after other scenes' immutable pins. Refresh for
    /// every scene: a preceding submission in this update may consume headroom.
    pub fn source_cache_bytes(&self, scene: Entity) -> Option<u64> {
        match self.backend_for(scene) {
            SogRendererBackend::Snapshot => None,
            SogRendererBackend::GpuResident => Some(
                self.gpu
                    .source_headroom(scene, self.budget_for(scene).source_bytes),
            ),
            SogRendererBackend::Packed => Some(
                self.packed
                    .source_headroom(scene, self.budget_for(scene).source_bytes),
            ),
        }
    }

    /// False means keep the previous complete snapshot and wait for this scene.
    pub fn poll(&mut self, state: &mut SnapshotState) -> bool {
        let Some(pending) = state.pending else {
            return true;
        };
        if !self.ready.0.lock().unwrap().contains(&pending) {
            return false;
        }
        self.commands.entity(pending).insert(Visibility::Inherited);
        if let Some((SogRendererBackend::Packed, _)) = state.owner {
            self.commands.queue(move |world: &mut World| {
                if let Some(mut compact) =
                    world.get_mut::<bevy_gaussian_splatting::render::compact::CompactCloud>(pending)
                {
                    compact.set_draw_enabled(true);
                }
            });
            self.packed.activated();
        }
        if let Some(old) = state.current.replace(pending) {
            self.commands.entity(old).despawn();
        }
        state.current_allocation = std::mem::take(&mut state.pending_allocation);
        state.pending = None;
        self.selections.ready_generations += 1;
        if let Some((SogRendererBackend::GpuResident, scene)) = state.owner {
            self.gpu.activated(scene);
        }
        true
    }

    /// Empty visibility retires both generations and their GPU ownership.
    pub fn clear(&mut self, state: &mut SnapshotState) {
        state.current_allocation = SnapshotAllocation::default();
        state.pending_allocation = SnapshotAllocation::default();
        for entity in [state.current.take(), state.pending.take()]
            .into_iter()
            .flatten()
        {
            self.commands.entity(entity).despawn();
        }
        if let Some((backend, scene)) = state.owner.take() {
            self.selections.counts.remove(&scene);
            if backend == SogRendererBackend::GpuResident {
                self.gpu.remove(scene, &self.transfers);
            }
            if backend == SogRendererBackend::Packed {
                self.packed.remove(scene, &self.transfers);
            }
        }
    }

    /// Stage a replacement generation while retaining displayed ownership.
    pub fn submit<'a>(
        &mut self,
        state: &mut SnapshotState,
        scene: Entity,
        camera: Entity,
        ranges: &[SplatRange],
        source: impl Fn(usize) -> Option<&'a DecodedChunk>,
    ) -> Result<Submission, String> {
        let budget = self.budget_for(scene);
        let compaction = self.compaction_for(scene);
        let projection_cache = self.projection_for(scene);
        if state.is_pending() {
            return Err("a snapshot submission is already pending".into());
        }
        if state
            .owner
            .is_some_and(|owner| owner != (self.backend_for(scene), scene))
        {
            return Err("renderer backend/scene cannot change while a state owns resources".into());
        }
        let count = ranges
            .iter()
            .try_fold(0usize, |sum, range| sum.checked_add(range.count));
        if count.is_none_or(|count| count == 0 || count > self.splat_limit_for(scene)) {
            if self.backend_for(scene) == SogRendererBackend::Packed {
                self.packed.cancel_preparation(scene);
            }
            return Err("selection is empty or exceeds the GPU storage binding limit".into());
        }
        let count = count.expect("validated selection count");
        if self.backend_for(scene) == SogRendererBackend::Packed {
            let cpu = self.cpu_timings();
            let cache_trace = self.cache_trace.as_deref().cloned().unwrap_or_default();
            let view = self
                .render_entities
                .get(camera)
                .map_err(|_| "packed camera has not reached render world")?
                .id();
            let pending = self
                .packed
                .submit(
                    scene,
                    view,
                    ranges,
                    source,
                    &budget,
                    self.device
                        .as_deref()
                        .ok_or("packed renderer requires RenderDevice")?,
                    self.queue
                        .as_deref()
                        .ok_or("packed renderer requires RenderQueue")?,
                    &self.clouds,
                    &self.sorted,
                    &mut self.commands,
                    &self.transfers,
                    CloudSettings {
                        raster_profile: GaussianRasterProfile::NormalizedGaussian,
                        visibility_compaction: compaction,
                        ..default()
                    },
                    &cpu,
                    projection_cache,
                    self.frame.as_ref().map(|f| f.0),
                    &cache_trace,
                )
                .inspect_err(|_| self.packed.cancel_preparation(scene))?;
            // Record ownership for clear/despawn even before a draw is created.
            state.owner = Some((self.backend_for(scene), scene));
            let Some(pending) = pending else {
                return Ok(Submission::Preparing);
            };
            self.commands
                .entity(camera)
                .insert(GaussianCamera::default());
            self.selections.counts.insert(scene, count);
            self.selections.submitted_generations += 1;
            state.pending = Some(pending);
            state.owner = Some((self.backend_for(scene), scene));
            return Ok(Submission::Submitted);
        }
        if self.backend_for(scene) == SogRendererBackend::GpuResident {
            // The pinned upstream sorter shares one cloud's entries between views.
            // Explicitly reject unsupported Gaussian multi-view configurations.
            if self.cameras.get(camera).is_err()
                || self.cameras.iter().any(|(entity, c, gaussian)| {
                    (entity == camera && c.order != 0) || (gaussian && entity != camera)
                })
            {
                return Err("GPU SOG adapter supports one Gaussian camera with order zero".into());
            }
            let device = self
                .device
                .as_ref()
                .ok_or("GPU SOG renderer requires RenderDevice")?;
            let cpu = self.cpu_timings();
            let plan = cpu.measure(CpuStage::SelectionPlan, || {
                gpu::SelectionPlan::new(ranges, source, self.splat_limit_for(scene))
            })?;
            let pending = self.gpu.submit(
                scene,
                plan,
                &budget,
                device,
                &self.clouds,
                &self.sorted,
                &mut self.commands,
                &self.transfers,
                &cpu,
            )?;
            self.commands
                .entity(camera)
                .insert(GaussianCamera::default());
            self.commands.entity(pending).insert(CloudSettings {
                raster_profile: GaussianRasterProfile::NormalizedGaussian,
                visibility_compaction: compaction,
                ..default()
            });
            self.selections.counts.insert(scene, count);
            self.selections.submitted_generations += 1;
            state.pending = Some(pending);
            state.owner = Some((self.backend_for(scene), scene));
            return Ok(Submission::Submitted);
        }
        let (cloud, bounds) = self.cpu_timings().measure(CpuStage::SnapshotCompose, || {
            let cloud = compose::compose(ranges, source)?;
            let bounds = bounds::cloud_bounds(&cloud, count)?;
            Ok::<_, String>((cloud, bounds))
        })?;
        let cpu_bytes = cloud.position_visibility.capacity() as u64 * gpu::strides()[0]
            + cloud.spherical_harmonic.capacity() as u64 * gpu::strides()[1]
            + cloud.rotation.capacity() as u64 * gpu::strides()[2]
            + cloud.scale_opacity.capacity() as u64 * gpu::strides()[3];
        state.pending_allocation = SnapshotAllocation {
            rows: cloud_rows(&cloud),
            cpu_bytes,
        };
        self.commands
            .entity(camera)
            .insert(GaussianCamera::default());
        state.pending = Some(
            self.commands
                .spawn((
                    readiness::Snapshot,
                    bounds,
                    Visibility::Hidden,
                    PlanarGaussian3dHandle(self.clouds.add(cloud)),
                    CloudSettings {
                        gaussian_mode: GaussianMode::Gaussian3d,
                        raster_profile: GaussianRasterProfile::NormalizedGaussian,
                        visibility_compaction: compaction,
                        ..default()
                    },
                    ChildOf(scene),
                    Name::new("StreamedSog snapshot"),
                ))
                .id(),
        );
        state.owner = Some((self.backend_for(scene), scene));
        self.selections.counts.insert(scene, count);
        self.selections.submitted_generations += 1;
        Ok(Submission::Submitted)
    }
}

/// Return the immutable expanded row cost.
pub fn source_bytes_per_splat() -> u64 {
    gpu::strides().iter().sum()
}

/// Every planar attribute is its own storage binding; account for padded rows.
pub fn gpu_splat_limit(binding_bytes: u64) -> usize {
    let g = Gaussian3d::default();
    let stride = [
        std::mem::size_of_val(&g.position_visibility),
        std::mem::size_of_val(&g.spherical_harmonic),
        std::mem::size_of_val(&g.rotation),
        std::mem::size_of_val(&g.scale_opacity),
    ]
    .into_iter()
    .max()
    .unwrap();
    ((binding_bytes / stride as u64).min(usize::MAX as u64) as usize / 32) * 32
}

#[cfg(test)]
mod tests;
