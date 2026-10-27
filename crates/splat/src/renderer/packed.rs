//! Application-owned packed source cache and bounded per-scene working slots.
//! Immutable uploads may originate in main update; working-buffer writes happen
//! only in the render schedule. Arc ownership prevents reuse before retirement.
use super::{
    DecodedChunk, SplatRange,
    gpu::{GpuBudget, GpuStatistics, GpuTransfers, scratch_bytes},
    source::{PackedSourceDescriptor, SourceData},
};
use bevy::{
    camera::primitives::Aabb,
    platform::time::Instant,
    prelude::*,
    render::{
        render_resource::*,
        renderer::{RenderDevice, RenderQueue, WgpuWrapper},
    },
};
use bevy_gaussian_splatting::{
    CloudSettings, PlanarGaussian3d, PlanarGaussian3dHandle,
    render::{
        compact::CompactCloud,
        sog::{
            GpuSogSource, GpuSogUpload,
            view::SourceRange,
            work::{BakeRange, CompactBaker, CompactWork},
        },
    },
    sort::{GpuSortedEntry, SortedEntries, SortedEntriesHandle},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
    time::Duration,
};

type Group = (bool, usize);

struct Source {
    group: Group,
    gpu: Arc<WgpuWrapper<GpuSogSource>>,
    used: u64,
    bounds: (Vec3, Vec3),
    cpu_recovery: Weak<bevy_gaussian_splatting::io::sog_packed::PackedSog>,
}

fn source_bounds(data: &bevy_gaussian_splatting::io::sog_packed::PackedSog) -> (Vec3, Vec3) {
    // Signed-log centres plus the largest three-sigma axis support. Cache this
    // small result with the GPU source before releasing its CPU texture data.
    let unpack = |v: f32| v.signum() * v.abs().exp_m1();
    let radius = data
        .scale_codebook()
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max)
        .exp()
        * 3.;
    (
        Vec3::from_array(data.means_min().map(unpack)) - Vec3::splat(radius),
        Vec3::from_array(data.means_max().map(unpack)) + Vec3::splat(radius),
    )
}
struct Upload {
    gpu: WgpuWrapper<GpuSogUpload>,
    group: Group,
    palette_bytes: u64,
    owns_palette: bool,
}
impl std::ops::Deref for Upload {
    type Target = GpuSogUpload;
    fn deref(&self) -> &Self::Target {
        &self.gpu
    }
}
impl std::ops::DerefMut for Upload {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gpu
    }
}

#[derive(Default)]
struct Footprint {
    bytes: u64,
    palettes: HashSet<(Entity, Group)>,
}
impl Footprint {
    fn add(&mut self, scene: Entity, group: Group, exclusive: u64, palette: u64) {
        self.bytes = self.bytes.saturating_add(exclusive);
        if self.palettes.insert((scene, group)) {
            self.bytes = self.bytes.saturating_add(palette);
        }
    }
}
fn palette_bytes(data: &bevy_gaussian_splatting::io::sog_packed::PackedSog) -> u64 {
    data.palette_texture_bytes() + 4096 + if data.higher_order().is_none() { 4 } else { 0 }
}

struct Slot {
    work: Arc<WgpuWrapper<CompactWork>>,
    cloud: Handle<PlanarGaussian3d>,
    sorted: Handle<SortedEntries>,
    entries: Buffer,
    indirect: Buffer,
    projected: Option<Buffer>,
    capacity: usize,
}
#[derive(Default)]
struct Scene {
    slots: [Option<Slot>; 2],
    next: usize,
    count: usize,
}
#[derive(Resource, Default)]
pub struct PackedResident {
    sources: HashMap<(Entity, usize), Source>,
    /// Coverage the whole-file scheduler must retain even while finer ranges
    /// are displayed. These IDs carry no extra allocation or CPU texel owner.
    fixed_sources: HashMap<Entity, HashSet<usize>>,
    /// Successfully admitted candidates still uploading over multiple frames.
    /// Completed files need ownership until CompactCloud acquires their Arcs.
    preparing_sources: HashMap<Entity, HashSet<usize>>,
    uploads: HashMap<(Entity, usize), Upload>,
    upload_frame: Option<u32>,
    upload_remaining: usize,
    upload_spent: Duration,
    allocation_remaining: usize,
    scenes: HashMap<Entity, Scene>,
    baker: Option<Arc<WgpuWrapper<CompactBaker>>>,
    pub statistics: GpuStatistics,
}

/// Return the bytes required by a selected packed generation.
pub(super) fn selected_bytes(count: usize) -> u64 {
    count as u64 * 32 + 16
}
/// Bound selected rows by the allocation budget and binding limit.
pub(super) fn selected_limit(budget: u64, binding: usize) -> usize {
    selected_limit_with_projection(budget, binding, false)
}

fn selected_bytes_with_projection(count: usize, projected: bool) -> u64 {
    selected_bytes(count)
        + if projected {
            count as u64 * bevy_gaussian_splatting::render::projected::RECORD_BYTES
        } else {
            0
        }
}

/// Include projection-cache allocations in the selected-row ceiling.
pub(super) fn selected_limit_with_projection(
    budget: u64,
    binding: usize,
    projected: bool,
) -> usize {
    let (mut low, mut high) = (0, binding / 32);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if (selected_bytes_with_projection(mid * 32, projected) + scratch_bytes(mid * 32))
            .saturating_mul(2)
            <= budget
        {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low * 32
}

fn slot_capacity(
    old: usize,
    requested: usize,
    active_bytes: u64,
    budget: u64,
    projected: bool,
) -> Option<usize> {
    // Reuse the high-water slot normally, but do not let historical capacity
    // prevent a smaller replacement after a configured memory reduction.
    [old.max(requested), requested]
        .into_iter()
        .find(|&capacity| {
            selected_bytes_with_projection(capacity, projected)
                + scratch_bytes(capacity)
                + active_bytes
                <= budget
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_cache_is_charged_for_both_transition_slots() {
        let count = 1024;
        let base = selected_bytes(count) + scratch_bytes(count);
        let projection = count as u64 * 32;
        assert_eq!(
            selected_limit_with_projection(2 * (base + projection), count, true),
            count
        );
        assert!(selected_limit_with_projection(2 * (base + projection) - 1, count, true) < count);
        assert!(selected_limit_with_projection(2 * base, count, true) < count);
        assert_eq!(
            selected_limit_with_projection(2 * base, count, false),
            count
        );
    }

    #[test]
    fn lowered_budget_reclaims_inactive_slot_high_water_capacity() {
        let old = 1024;
        let requested = 32;
        let active_bytes = selected_bytes(old) + scratch_bytes(old);
        let budget = active_bytes + selected_bytes(requested) + scratch_bytes(requested);
        assert_eq!(
            slot_capacity(old, requested, active_bytes, budget, false),
            Some(requested)
        );
        assert_eq!(
            slot_capacity(old, requested, active_bytes, budget - 1, false),
            None
        );
        assert_eq!(
            slot_capacity(old, requested, active_bytes, u64::MAX, false),
            Some(old)
        );
    }
}
impl PackedResident {
    /// Retain sources owned by a displayed or pending generation.
    pub(super) fn pin_sources(&mut self, scene: Entity, files: impl IntoIterator<Item = usize>) {
        let pins = self.fixed_sources.entry(scene).or_default();
        pins.clear();
        pins.extend(files);
    }

    fn is_pinned(&self, key: &(Entity, usize)) -> bool {
        self.fixed_sources
            .get(&key.0)
            .is_some_and(|files| files.contains(&key.1))
            || self
                .preparing_sources
                .get(&key.0)
                .is_some_and(|files| files.contains(&key.1))
    }
    /// Retire partial allocations for a superseded generation.
    pub(super) fn cancel_preparation(&mut self, scene: Entity) {
        self.preparing_sources.remove(&scene);
        self.uploads.retain(|(owner, _), _| *owner != scene);
        self.refresh();
    }
    /// No GPU ownership is transferred to the selector. This permits eviction
    /// under pressure; a descriptor is invalidated on the next main-world pass.
    pub(super) fn sync_source(
        &self,
        scene: Entity,
        file: usize,
        ready: &mut Option<DecodedChunk>,
    ) -> bool {
        if let Some(source) = self.sources.get(&(scene, file)) {
            let count = source.gpu.count() as usize;
            *ready = Some(DecodedChunk {
                count,
                cloud: SourceData::ResidentPacked(PackedSourceDescriptor {
                    count,
                    group: source.group,
                    gpu_bytes: source.gpu.exclusive_bytes() + source.gpu.palette().resident_bytes(),
                }),
            });
        } else if ready
            .as_ref()
            .is_some_and(|chunk| matches!(chunk.cloud, SourceData::ResidentPacked(_)))
        {
            *ready = None;
            return true;
        }
        false
    }
    /// Report allocated projection-cache bytes.
    pub fn projection_bytes(&self) -> u64 {
        self.scenes
            .values()
            .flat_map(|s| s.slots.iter().flatten())
            .filter_map(|s| s.projected.as_ref())
            .map(|buffer| buffer.size())
            .sum()
    }
    /// Report selection allocations owned by one scene.
    pub fn scene_memory(&self, entity: Entity) -> super::SelectionMemory {
        let mut memory = super::SelectionMemory::default();
        if let Some(scene) = self.scenes.get(&entity) {
            for slot in scene.slots.iter().flatten() {
                memory.selected_gpu_bytes +=
                    selected_bytes_with_projection(slot.capacity, slot.projected.is_some());
                memory.radix_reserved_bytes += scratch_bytes(slot.capacity);
                memory.generations += 1;
            }
        }
        memory
    }
    fn footprint(
        &self,
        keep: impl Fn(&(Entity, usize), &Source) -> bool,
        keep_upload: impl Fn(&(Entity, usize)) -> bool,
    ) -> Footprint {
        let mut out = Footprint::default();
        for (key, source) in &self.sources {
            if keep(key, source) {
                out.add(
                    key.0,
                    source.group,
                    source.gpu.exclusive_bytes(),
                    source.gpu.palette().resident_bytes(),
                );
            }
        }
        for (key, upload) in &self.uploads {
            if keep_upload(key) {
                let exclusive = upload.resident_bytes()
                    - if upload.owns_palette {
                        upload.palette_bytes
                    } else {
                        0
                    };
                out.add(key.0, upload.group, exclusive, upload.palette_bytes);
            }
        }
        out
    }
    fn needed_bytes(
        &self,
        scene: Entity,
        inputs: &HashMap<usize, Option<&bevy_gaussian_splatting::io::sog_packed::PackedSog>>,
        groups: &HashMap<usize, Group>,
        keep: impl Fn(&(Entity, usize), &Source) -> bool,
    ) -> u64 {
        let mut out = self.footprint(keep, |_| true);
        for (&id, data) in inputs {
            if !self.sources.contains_key(&(scene, id)) && !self.uploads.contains_key(&(scene, id))
            {
                let data = data.expect("missing source must have validated CPU staging");
                let palette = palette_bytes(data);
                out.add(
                    scene,
                    groups[&id],
                    GpuSogSource::required_bytes(data) - palette,
                    palette,
                );
            }
        }
        out.bytes
    }
    /// Report immutable source bytes retained for one scene.
    pub fn scene_source_bytes(&self, entity: Entity) -> u64 {
        self.footprint(|key, _| key.0 == entity, |key| key.0 == entity)
            .bytes
    }
    /// Return the source budget available to this scene.
    pub(super) fn source_headroom(&self, scene: Entity, budget: u64) -> u64 {
        budget.saturating_sub(
            self.footprint(
                |key, source| {
                    key.0 != scene && (self.is_pinned(key) || Arc::strong_count(&source.gpu) > 1)
                },
                |key| key.0 != scene,
            )
            .bytes,
        )
    }
    fn refresh(&mut self) {
        self.statistics.resident_bytes = self.footprint(|_, _| true, |_| true).bytes;
        self.statistics.selected_buffer_bytes = 0;
        self.statistics.radix_scratch_bytes_reserved = 0;
        self.statistics.selected_slot_capacity = 0;
        self.statistics.selected_count = self.scenes.values().map(|s| s.count).sum();
        for scene in self.scenes.values() {
            for slot in scene.slots.iter().flatten() {
                self.statistics.selected_buffer_bytes +=
                    selected_bytes_with_projection(slot.capacity, slot.projected.is_some());
                self.statistics.radix_scratch_bytes_reserved += scratch_bytes(slot.capacity);
                self.statistics.selected_slot_capacity += slot.capacity;
            }
        }
    }
    /// Record a renderer-ready generation activation.
    pub(super) fn activated(&mut self) {
        self.statistics.ready_generations += 1;
    }
    /// Retire scene-owned resources through the renderer transfer queue.
    pub(super) fn remove(&mut self, entity: Entity, transfers: &GpuTransfers) {
        self.fixed_sources.remove(&entity);
        self.preparing_sources.remove(&entity);
        self.uploads.retain(|(owner, _), _| *owner != entity);
        if let Some(scene) = self.scenes.remove(&entity) {
            if let Ok(mut transfers) = transfers.0.lock() {
                for slot in scene.slots.into_iter().flatten() {
                    transfers
                        .compact_sorted
                        .retain(|publication| publication.sorted.id() != slot.sorted.id());
                    transfers.retire.push((slot.cloud.id(), slot.sorted.id()));
                }
            }
        }
        // GPU draw generations can outlive main-world despawn until extraction.
        // Retain and account for those source handles until their owners retire.
        self.sources
            .retain(|(owner, _), source| *owner != entity || Arc::strong_count(&source.gpu) > 1);
        self.refresh();
    }

    /// Stage a replacement generation while retaining displayed ownership.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn submit<'a>(
        &mut self,
        scene: Entity,
        view: Entity,
        ranges: &[SplatRange],
        source: impl Fn(usize) -> Option<&'a DecodedChunk>,
        budget: &GpuBudget,
        device: &RenderDevice,
        queue: &RenderQueue,
        clouds: &Assets<PlanarGaussian3d>,
        sorted: &Assets<SortedEntries>,
        commands: &mut Commands,
        transfers: &GpuTransfers,
        settings: CloudSettings,
        cpu: &crate::diagnostics::sog_cpu::SogCpuTimings,
        projected: bool,
        frame: Option<u32>,
        cache_trace: &crate::diagnostics::sog_cache::SogCacheTrace,
    ) -> Result<Option<Entity>, String> {
        // Supersession releases only this candidate's preparation ownership.
        // Existing draw generations and required coarse coverage remain pinned.
        self.preparing_sources.remove(&scene);
        let plan_timer = cpu.start(crate::diagnostics::sog_cpu::CpuStage::SelectionPlan);
        if !bevy_gaussian_splatting::render::compact::supports(&settings) {
            return Err("packed rendering requires supported 3D radix cloud settings".into());
        }
        let count = ranges
            .iter()
            .try_fold(0usize, |sum, r| sum.checked_add(r.count))
            .ok_or("packed selection overflow")?;
        let padded = count.checked_add(31).ok_or("packed padding overflow")? / 32 * 32;
        if count == 0 || count > u32::MAX as usize {
            return Err("invalid packed count".into());
        }
        let mut inputs = HashMap::new();
        let mut groups = HashMap::new();
        let mut incoming = 0u64;
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for r in ranges.iter().filter(|r| r.count > 0) {
            let chunk = source(r.file).ok_or("packed source not ready")?;
            let data = match &chunk.cloud {
                SourceData::ResidentPacked(info) => {
                    let resident = self
                        .sources
                        .get(&(scene, r.file))
                        .ok_or("packed GPU source no longer resident")?;
                    if info.count != chunk.count || resident.gpu.count() as usize != chunk.count {
                        return Err("packed resident source identity changed".into());
                    }
                    None
                }
                _ => Some(chunk.cloud.packed()?),
            };
            let group = chunk.cloud.palette_group(r.file);
            groups.insert(r.file, group);
            if r.offset
                .checked_add(r.count)
                .is_none_or(|end| end > chunk.count)
                || data.is_some_and(|data| chunk.count != data.count() as usize)
            {
                return Err("packed range exceeds immutable source".into());
            }
            if inputs.insert(r.file, data).is_none() {
                if let Some(old) = self.sources.get(&(scene, r.file)) {
                    if old.gpu.count() as usize != chunk.count || old.group != group {
                        return Err("packed source identity changed".into());
                    }
                } else if let Some(upload) = self.uploads.get(&(scene, r.file)) {
                    if upload.count() as usize != chunk.count || upload.group != group {
                        return Err("packed uploading source identity changed".into());
                    }
                } else {
                    incoming = incoming
                        .checked_add(GpuSogSource::required_bytes(
                            data.ok_or("missing packed CPU staging")?,
                        ))
                        .ok_or("packed size overflow")?;
                }
                let bounds = match self.sources.get(&(scene, r.file)) {
                    Some(source) => source.bounds,
                    None => source_bounds(data.ok_or("missing packed CPU staging")?),
                };
                min = min.min(bounds.0);
                max = max.max(bounds.1);
            }
        }
        if !min.is_finite() || !max.is_finite() {
            return Err("non-finite packed source bounds".into());
        }
        let index = self.scenes.get(&scene).map_or(0, |s| s.next);
        let old = self
            .scenes
            .get(&scene)
            .and_then(|s| s.slots[index].as_ref());
        if old.is_some_and(|s| Arc::strong_count(&s.work) > 1) {
            // Render extraction can still hold the previous owner for a frame.
            // Keep preparation pending so reactive rendering advances retirement
            // and retries; reporting an error here can strand it until idle wake.
            return Ok(None);
        }
        let other = self
            .scenes
            .get(&scene)
            .and_then(|s| s.slots[1 - index].as_ref())
            .map_or(0, |s| {
                selected_bytes_with_projection(s.capacity, s.projected.is_some())
                    + scratch_bytes(s.capacity)
            });
        let capacity = slot_capacity(
            old.map_or(0, |s| s.capacity),
            padded,
            other,
            budget.selected_bytes_per_scene,
            projected,
        )
        .ok_or("GPU selected slot budget exceeded")?;
        if projected
            && (capacity as u64 * bevy_gaussian_splatting::render::projected::RECORD_BYTES
                > u64::from(device.limits().max_storage_buffer_binding_size)
                    .min(device.limits().max_buffer_size))
        {
            return Err("projection cache exceeds device binding".into());
        }
        // Abandoned partial sources have no render owners or CPU clones. Cancel
        // them before admission and reclaim their complete reserved GPU payload.
        self.uploads
            .retain(|(owner, file), _| *owner != scene || inputs.contains_key(file));
        self.refresh();
        let can_evict = |key: &(Entity, usize), source: &Source| {
            !self.is_pinned(key)
                && Arc::strong_count(&source.gpu) == 1
                && !(key.0 == scene && inputs.contains_key(&key.1))
        };
        let mut evictable: Vec<_> = self
            .sources
            .iter()
            .filter(|(key, source)| can_evict(key, source))
            .map(|(key, source)| (*key, source.used, source.cpu_recovery.strong_count() > 0))
            .collect();
        // Prefer a re-upload over a fresh download/decode. This is only a
        // victim-order preference: pins and exact source admission still win.
        // Weak references expire immediately when CPU admission trims a backup;
        // no stale residency flag or extra strong owner can hide that loss.
        evictable.sort_by_key(|(key, used, recoverable)| (!recoverable, *used, key.0, key.1));
        if self.needed_bytes(scene, &inputs, &groups, |key, source| {
            !can_evict(key, source)
        }) > budget.source_bytes
        {
            return Err("GPU source budget is pinned by packed render generations".into());
        }
        self.preparing_sources
            .insert(scene, inputs.keys().copied().collect());
        drop(plan_timer);
        let upload_timer = (incoming > 0 || self.uploads.keys().any(|(owner, _)| *owner == scene))
            .then(|| cpu.start(crate::diagnostics::sog_cpu::CpuStage::SourceUploadPrepare));
        if self.baker.is_none() {
            self.baker = Some(Arc::new(WgpuWrapper::new(CompactBaker::new(
                device.wgpu_device(),
            )?)));
        }
        let baker = self
            .baker
            .as_ref()
            .ok_or("packed baker unavailable")?
            .clone();
        for (key, _, _) in evictable {
            let needed = self.needed_bytes(scene, &inputs, &groups, |_, _| true);
            if needed <= budget.source_bytes {
                break;
            }
            if let Some(source) = self.sources.get(&key) {
                cache_trace.scene(key.0).gpu_eviction(
                    key.1,
                    source.gpu.exclusive_bytes(),
                    self.statistics.submitted_generations + 1,
                    || {
                        use crate::diagnostics::sog_cache::{
                            GPU_EVIDENCE_LIMIT, GpuAdmission, GpuSourceOwner,
                        };
                        // Capture before removal, without cloning any source Arc.
                        // Always include the victim even when the list truncates.
                        let mut sources: Vec<_> = std::iter::once((&key, source))
                            .chain(self.sources.iter().filter(|(id, _)| **id != key))
                            .take(GPU_EVIDENCE_LIMIT)
                            .map(|(id, source)| {
                                let fixed = self
                                    .fixed_sources
                                    .get(&id.0)
                                    .is_some_and(|files| files.contains(&id.1));
                                let preparing = self
                                    .preparing_sources
                                    .get(&id.0)
                                    .is_some_and(|files| files.contains(&id.1));
                                let requested = id.0 == scene && inputs.contains_key(&id.1);
                                let external_owners =
                                    Arc::strong_count(&source.gpu).saturating_sub(1);
                                GpuSourceOwner {
                                    scene: id.0.to_bits().to_string(),
                                    source: id.1,
                                    last_used_generation: source.used,
                                    exclusive_bytes: source.gpu.exclusive_bytes(),
                                    external_owners,
                                    fixed,
                                    preparing,
                                    requested,
                                    evictable: !fixed
                                        && !preparing
                                        && !requested
                                        && external_owners == 0,
                                }
                            })
                            .collect();
                        sources.sort_unstable_by(|a, b| {
                            (&a.scene, a.source).cmp(&(&b.scene, b.source))
                        });
                        let mut requested: Vec<_> =
                            inputs.keys().copied().take(GPU_EVIDENCE_LIMIT).collect();
                        requested.sort_unstable();
                        GpuAdmission {
                            submitting_scene: scene.to_bits().to_string(),
                            budget_bytes: budget.source_bytes,
                            resident_bytes_before: self.footprint(|_, _| true, |_| true).bytes,
                            needed_bytes_before: needed,
                            requested,
                            sources,
                            complete: self.sources.len() <= GPU_EVIDENCE_LIMIT
                                && inputs.len() <= GPU_EVIDENCE_LIMIT,
                        }
                    },
                );
            }
            self.sources.remove(&key);
        }
        self.refresh();
        let generation = self.statistics.submitted_generations + 1;
        // One shared allowance across all scenes and retry calls in a frame.
        // The tiny LUT/uniform upload is also charged. Main texture rows remain
        // on their original CPU owner; no unaccounted staging clone is retained.
        if frame.is_none() || self.upload_frame != frame {
            self.upload_frame = frame;
            self.upload_remaining = 4 * 1024 * 1024;
            self.upload_spent = Duration::ZERO;
            self.allocation_remaining = 1;
        }
        let upload_started = Instant::now();
        let time_available =
            |spent: Duration| spent + upload_started.elapsed() < Duration::from_millis(2);
        self.scenes.entry(scene).or_default();
        let mut files: Vec<_> = inputs.keys().copied().collect();
        // A superseding selection can introduce an earlier page ID while the
        // palette owner is still uploading. Finish retained uploads first so
        // new pages share that palette instead of allocating a duplicate.
        files.sort_unstable_by_key(|file| (!self.uploads.contains_key(&(scene, *file)), *file));
        for file in files {
            if !time_available(self.upload_spent) {
                break;
            }
            if self.sources.contains_key(&(scene, file)) {
                continue;
            }
            let data = inputs[&file].ok_or("missing packed CPU staging")?;
            if !self.uploads.contains_key(&(scene, file)) {
                let group = groups[&file];
                let shared = self
                    .sources
                    .iter()
                    .find(|((owner, _), old)| *owner == scene && old.group == group)
                    .map(|(_, old)| old.gpu.palette().clone());
                let metadata_bytes = if shared.is_some() { 48 } else { 4144 };
                if self.allocation_remaining == 0 || self.upload_remaining < metadata_bytes {
                    break;
                }
                let layout = GpuSogSource::layout(device.wgpu_device());
                let palette_bytes = palette_bytes(data);
                let required = GpuSogSource::required_bytes(data);
                let upload = if let Some(palette) = shared.as_ref() {
                    GpuSogSource::begin_upload_with_palette(
                        device.wgpu_device(),
                        queue,
                        &layout,
                        data,
                        palette,
                        required - palette_bytes,
                    )
                } else {
                    GpuSogSource::begin_upload(device.wgpu_device(), queue, &layout, data, required)
                }
                .map_err(|error| format!("packed source upload: {error:?}"))?;
                self.statistics.resident_bytes += upload.resident_bytes();
                self.statistics.uploaded_bytes += metadata_bytes as u64;
                self.upload_remaining -= metadata_bytes;
                self.allocation_remaining -= 1;
                self.uploads.insert(
                    (scene, file),
                    Upload {
                        gpu: WgpuWrapper::new(upload),
                        group,
                        palette_bytes,
                        owns_palette: shared.is_none(),
                    },
                );
            }
            let upload = self
                .uploads
                .get_mut(&(scene, file))
                .expect("reserved upload");
            // Cooperative time cutoff plus a hard byte ceiling. Small calls
            // avoid the large JS/WASM transfer bursts even on slower devices.
            while !upload.is_complete() && time_available(self.upload_spent) {
                let written = upload.write(queue, data, self.upload_remaining.min(512 * 1024));
                if written == 0 {
                    break;
                }
                self.upload_remaining -= written;
                self.statistics.uploaded_bytes += written as u64;
            }
            if upload.is_complete() {
                let upload = self
                    .uploads
                    .remove(&(scene, file))
                    .expect("complete upload");
                let group = upload.group;
                let gpu = upload
                    .gpu
                    .into_inner()
                    .finish()
                    .ok()
                    .expect("complete source");
                cache_trace.scene(scene).record(
                    crate::diagnostics::sog_cache::Kind::Uploaded,
                    file,
                    gpu.exclusive_bytes(),
                    None,
                    generation,
                );
                self.sources.insert(
                    (scene, file),
                    Source {
                        group,
                        gpu: Arc::new(WgpuWrapper::new(gpu)),
                        used: generation,
                        bounds: source_bounds(data),
                        cpu_recovery: source(file)
                            .expect("validated source")
                            .cloud
                            .packed_recovery(),
                    },
                );
            } else {
                break;
            }
        }
        self.upload_spent += upload_started.elapsed();
        if inputs
            .keys()
            .any(|file| !self.sources.contains_key(&(scene, *file)))
        {
            return Ok(None);
        }
        drop(upload_timer);
        let _selection_timer =
            cpu.start(crate::diagnostics::sog_cpu::CpuStage::SelectionCopyPrepare);
        let scene_data = self.scenes.entry(scene).or_default();
        if scene_data.slots[index]
            .as_ref()
            .is_none_or(|s| s.capacity != capacity || s.projected.is_some() != projected)
        {
            let old = scene_data.slots[index].take();
            let cloud = old
                .as_ref()
                .map_or_else(|| clouds.reserve_handle(), |s| s.cloud.clone());
            let sorted = old
                .as_ref()
                .map_or_else(|| sorted.reserve_handle(), |s| s.sorted.clone());
            drop(old);
            let buffer = |size, usage, label| {
                device.create_buffer(&BufferDescriptor {
                    label: Some(label),
                    size,
                    usage,
                    mapped_at_creation: false,
                })
            };
            scene_data.slots[index] = Some(Slot {
                work: Arc::new(WgpuWrapper::new(CompactWork::new(
                    device.wgpu_device(),
                    capacity as u32,
                    capacity as u64 * 24,
                )?)),
                cloud,
                sorted,
                capacity,
                projected: projected.then(|| {
                    buffer(
                        capacity as u64 * bevy_gaussian_splatting::render::projected::RECORD_BYTES,
                        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                        "packed projection slot",
                    )
                }),
                entries: buffer(
                    capacity as u64 * 8,
                    BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
                    "packed sort entries",
                ),
                indirect: buffer(
                    16,
                    BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
                    "packed draw indirect",
                ),
            });
            self.statistics.allocations += 1;
        } else {
            self.statistics.reuses += 1;
        }
        let slot = scene_data.slots[index]
            .as_ref()
            .ok_or("packed slot unavailable")?;
        let mut destination = 0;
        let mut selected = Vec::new();
        for r in ranges.iter().filter(|r| r.count > 0) {
            let resident = self
                .sources
                .get_mut(&(scene, r.file))
                .ok_or("packed upload missing")?;
            resident.used = generation;
            selected.push(SourceRange {
                source: resident.gpu.clone(),
                range: BakeRange {
                    source_start: r.offset as u32,
                    destination_start: destination,
                    count: r.count as u32,
                },
            });
            destination += r.count as u32;
        }
        let mut compact = CompactCloud::from_sources(
            slot.work.clone(),
            baker,
            selected,
            slot.indirect.clone(),
            view,
        )?;
        compact.set_color_refresh_angle(10.0)?;
        if let Some(previous) = &scene_data.slots[1 - index] {
            compact.reuse_work_from(&previous.work)?;
        }
        if let Some(buffer) = &slot.projected {
            compact.set_projection_cache(buffer.clone())?;
        }
        compact.set_draw_enabled(false);
        let mut publication = transfers
            .0
            .lock()
            .map_err(|_| "SOG publication mailbox poisoned")?;
        let entity = commands
            .spawn((
                super::readiness::Snapshot,
                super::readiness::GpuSnapshot,
                compact,
                Visibility::Hidden,
                PlanarGaussian3dHandle(slot.cloud.clone()),
                SortedEntriesHandle(slot.sorted.clone()),
                settings,
                Aabb::from_min_max(min, max),
                ChildOf(scene),
                Name::new("StreamedSog packed selection"),
            ))
            .id();
        publication
            .compact_sorted
            .push(super::gpu::CompactPublication {
                cloud: slot.cloud.id(),
                sorted: slot.sorted.clone(),
                capacity: slot.capacity,
                entries: GpuSortedEntry {
                    sorted_entry_buffer: slot.entries.clone(),
                    count,
                },
            });
        scene_data.next = 1 - index;
        scene_data.count = count;
        self.statistics.submitted_generations = generation;
        // The queued CompactCloud now owns every source, including before
        // Commands are applied. Preparation pins must not outlive that handoff.
        self.preparing_sources.remove(&scene);
        self.refresh();
        Ok(Some(entity))
    }
}

/// Retire allocations whose scene owners no longer exist.
pub(super) fn cleanup(
    mut packed: ResMut<PackedResident>,
    transfers: Res<GpuTransfers>,
    entities: Query<Entity>,
) {
    packed
        .fixed_sources
        .retain(|entity, _| entities.contains(*entity));
    let removed: Vec<_> = packed
        .scenes
        .keys()
        .filter(|e| !entities.contains(**e))
        .copied()
        .collect();
    for entity in removed {
        packed.remove(entity, &transfers);
    }
    let present: HashSet<_> = packed.scenes.keys().copied().collect();
    packed
        .sources
        .retain(|(scene, _), source| present.contains(scene) || Arc::strong_count(&source.gpu) > 1);
    packed.refresh();
}
