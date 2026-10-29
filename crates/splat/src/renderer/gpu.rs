//! Persistent scene-scoped source planes and two reusable selection slots.
//!
//! Immutable sources upload once. Selection copies stay encoded in the mailbox
//! until synchronized extraction, after the previous render frame has submitted.
//! Decoded source arrays never cross worlds.
use super::{DecodedChunk, SplatRange};
use bevy::{
    camera::primitives::Aabb,
    prelude::*,
    render::{
        render_resource::*,
        renderer::{RenderDevice, WgpuWrapper},
    },
};
use bevy_gaussian_splatting::{
    Gaussian3d, PlanarGaussian3d,
    gaussian::formats::planar_3d::PlanarStorageGaussian3d,
    sort::{GpuSortedEntry, SortedEntries},
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SogRendererBackend {
    Snapshot,
    GpuResident,
    #[default]
    Packed,
}

/// Source residency is global across scenes; slot budget is per scene, including
/// planar/sort buffers plus upstream radix scratch reservations).
#[derive(Resource)]
pub struct GpuBudget {
    pub source_bytes: u64,
    pub selected_bytes_per_scene: u64,
}
impl Default for GpuBudget {
    fn default() -> Self {
        Self {
            source_bytes: if cfg!(target_arch = "wasm32") {
                256
            } else {
                512
            } * 1024
                * 1024,
            selected_bytes_per_scene: 512 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GpuStatistics {
    pub uploaded_bytes: u64,
    /// Copy bytes encoded, including generations canceled before extraction.
    pub encoded_copy_bytes: u64,
    pub resident_bytes: u64,
    pub selected_buffer_bytes: u64,
    /// Budget reservation for upstream radix scratch at each slot capacity.
    pub radix_scratch_bytes_reserved: u64,
    pub allocations: u64,
    pub reuses: u64,
    pub selected_count: usize,
    /// Sum of padded high-water capacities across both slots of all scenes.
    pub selected_slot_capacity: usize,
    pub submitted_generations: u64,
    pub ready_generations: u64,
}

/// Return the four expanded plane strides.
pub(super) fn strides() -> [u64; 4] {
    let g = Gaussian3d::default();
    [
        std::mem::size_of_val(&g.position_visibility) as u64,
        std::mem::size_of_val(&g.spherical_harmonic) as u64,
        std::mem::size_of_val(&g.rotation) as u64,
        std::mem::size_of_val(&g.scale_opacity) as u64,
    ]
}

pub(super) struct SelectionPlan<'a> {
    pub count: usize,
    pub padded: usize,
    sources: HashMap<usize, &'a DecodedChunk>,
    ranges: Vec<SplatRange>,
}
impl<'a> SelectionPlan<'a> {
    /// Create the bounded state owned by this operation.
    pub(super) fn new(
        ranges: &[SplatRange],
        source: impl Fn(usize) -> Option<&'a DecodedChunk>,
        limit: usize,
    ) -> Result<Self, String> {
        let count = ranges
            .iter()
            .try_fold(0usize, |n, r| n.checked_add(r.count))
            .ok_or("selected count overflow")?;
        let padded = count.checked_add(31).ok_or("selected padding overflow")? / 32 * 32;
        if count == 0 || padded > limit {
            return Err("selection is empty or exceeds GPU budget".into());
        }
        let mut sources = HashMap::new();
        for range in ranges.iter().filter(|r| r.count != 0) {
            let chunk = source(range.file).ok_or("chunk not ready")?;
            let end = range
                .offset
                .checked_add(range.count)
                .ok_or("source range overflow")?;
            let c = chunk.cloud.expanded()?;
            if end > chunk.count
                || [
                    c.position_visibility.len(),
                    c.spherical_harmonic.len(),
                    c.rotation.len(),
                    c.scale_opacity.len(),
                ]
                .into_iter()
                .any(|n| n < chunk.count)
            {
                return Err("range exceeds decoded source planes".into());
            }
            for stride in strides() {
                (end as u64)
                    .checked_mul(stride)
                    .ok_or("source byte range overflow")?;
                (padded as u64)
                    .checked_mul(stride)
                    .ok_or("selected byte range overflow")?;
            }
            sources.insert(range.file, chunk);
        }
        Ok(Self {
            count,
            padded,
            sources,
            ranges: ranges.to_vec(),
        })
    }
}

struct Source {
    planes: [Buffer; 4],
    count: usize,
    bytes: u64,
    last_used: u64,
    bounds: Aabb,
}
#[derive(Clone)]
struct Slot {
    cloud_handle: Handle<PlanarGaussian3d>,
    sort_handle: Handle<SortedEntries>,
    cloud: PlanarStorageGaussian3d,
    sorted: GpuSortedEntry,
    capacity: usize,
}
impl Slot {
    fn bytes(&self) -> u64 {
        selected_bytes(self.capacity)
    }
}
#[derive(Default)]
struct Scene {
    slots: [Option<Slot>; 2],
    next: usize,
    selected_count: usize,
    pins: [HashSet<usize>; 2],
}

pub(super) struct Publication {
    /// Kept unsubmitted while main runs alongside the previous render frame.
    pub selection_copies: WgpuWrapper<CommandEncoder>,
    pub copy_bytes: u64,
    pub entity: Entity,
    pub scene: Entity,
    pub cloud_handle: Handle<PlanarGaussian3d>,
    pub sort_handle: Handle<SortedEntries>,
    pub cloud: PlanarStorageGaussian3d,
    pub sorted: GpuSortedEntry,
}
#[derive(Default)]
pub(super) struct Transfers {
    pub compact_sorted: Vec<CompactPublication>,
    /// Queue-submitted bytes, not GPU-completion measurements.
    pub submitted_copy_bytes: u64,
    pub publish: Vec<Publication>,
    pub retire: Vec<(AssetId<PlanarGaussian3d>, AssetId<SortedEntries>)>,
}
pub(super) struct CompactPublication {
    pub cloud: AssetId<PlanarGaussian3d>,
    pub sorted: Handle<SortedEntries>,
    pub entries: GpuSortedEntry,
    pub capacity: usize,
}
#[derive(Resource, Clone, Default)]
pub struct GpuTransfers(pub(super) Arc<Mutex<Transfers>>);
impl GpuTransfers {
    /// Report bytes submitted through the GPU transfer queue.
    pub fn submitted_copy_bytes(&self) -> u64 {
        self.0.lock().unwrap().submitted_copy_bytes
    }
}

#[derive(Resource, Default)]
pub struct GpuResident {
    scenes: HashMap<Entity, Scene>,
    sources: HashMap<(Entity, usize), Source>,
    pub statistics: GpuStatistics,
}
impl GpuResident {
    /// Observe capacities without retaining allocations or counting reservations.
    #[cfg(feature = "sog-gpu-timing")]
    pub(super) fn observe_buffers(&self, inventory: &mut super::telemetry::BufferInventory) {
        for source in self.sources.values() {
            for buffer in &source.planes {
                inventory.add(buffer, "expanded_source");
            }
        }
        for slot in self
            .scenes
            .values()
            .flat_map(|scene| scene.slots.iter().flatten())
        {
            for buffer in [
                &slot.cloud.position_visibility,
                &slot.cloud.spherical_harmonic,
                &slot.cloud.rotation,
                &slot.cloud.scale_opacity,
                &slot.cloud.draw_indirect_buffer,
            ] {
                inventory.add(buffer, "expanded_selection");
            }
            inventory.add(&slot.sorted.sorted_entry_buffer, "sorted_entries");
        }
    }

    /// Report selection allocations owned by one scene.
    pub fn scene_memory(&self, scene: Entity) -> super::SelectionMemory {
        let mut memory = super::SelectionMemory::default();
        if let Some(scene) = self.scenes.get(&scene) {
            for slot in scene.slots.iter().flatten() {
                memory.selected_gpu_bytes += slot.bytes();
                memory.radix_reserved_bytes += scratch_bytes(slot.capacity);
                memory.generations += 1;
            }
        }
        memory
    }

    /// Report immutable source bytes retained for one scene.
    pub fn scene_source_bytes(&self, scene: Entity) -> u64 {
        self.sources
            .iter()
            .filter(|((owner, _), _)| *owner == scene)
            .map(|(_, source)| source.bytes)
            .sum()
    }
    /// Unpinned LRU entries can be reclaimed atomically by submit. Both slots
    /// remain pinned until readiness acknowledges the replacement generation.
    pub(super) fn source_headroom(&self, scene: Entity, budget: u64) -> u64 {
        let other_pins = self
            .sources
            .iter()
            .filter(|((owner, file), _)| {
                *owner != scene
                    && self
                        .scenes
                        .get(owner)
                        .is_some_and(|s| s.pins.iter().any(|pins| pins.contains(file)))
            })
            .fold(0u64, |sum, (_, source)| sum.saturating_add(source.bytes));
        budget.saturating_sub(other_pins)
    }
    /// Retire scene-owned resources through the renderer transfer queue.
    pub(super) fn remove(&mut self, scene: Entity, transfers: &GpuTransfers) {
        if let Some(scene_data) = self.scenes.remove(&scene) {
            let mut transfers = transfers.0.lock().unwrap();
            transfers.publish.retain(|p| p.scene != scene);
            for slot in scene_data.slots.into_iter().flatten() {
                self.statistics.selected_buffer_bytes -= slot.bytes();
                self.statistics.selected_slot_capacity -= slot.capacity;
                self.statistics.radix_scratch_bytes_reserved -= scratch_bytes(slot.capacity);
                transfers
                    .retire
                    .push((slot.cloud_handle.id(), slot.sort_handle.id()));
            }
        }
        self.statistics.selected_count = self.scenes.values().map(|s| s.selected_count).sum();
        self.sources.retain(|(owner, _), source| {
            if *owner == scene {
                self.statistics.resident_bytes -= source.bytes;
                false
            } else {
                true
            }
        });
    }

    /// Stage a replacement generation while retaining displayed ownership.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn submit(
        &mut self,
        scene: Entity,
        plan: SelectionPlan,
        budget: &GpuBudget,
        device: &RenderDevice,
        clouds: &Assets<PlanarGaussian3d>,
        sorted: &Assets<SortedEntries>,
        commands: &mut Commands,
        transfers: &GpuTransfers,
        cpu: &crate::diagnostics::sog_cpu::SogCpuTimings,
    ) -> Result<Entity, String> {
        // Preflight all admission before any source upload, eviction or slot write.
        let mut incoming = 0u64;
        let mut incoming_bounds = HashMap::new();
        for (&file, chunk) in &plan.sources {
            if let Some(resident) = self.sources.get(&(scene, file)) {
                if resident.count != chunk.count {
                    return Err("immutable GPU source identity changed count".into());
                }
            } else {
                let bytes = (chunk.count as u64)
                    .checked_mul(strides().iter().sum())
                    .ok_or("source size overflow")?;
                if strides()
                    .iter()
                    .any(|stride| chunk.count as u64 * stride > device.limits().max_buffer_size)
                {
                    return Err("source exceeds GPU buffer limit".into());
                }
                incoming_bounds.insert(
                    file,
                    super::bounds::cloud_bounds(chunk.cloud.expanded()?, chunk.count)?,
                );
                incoming = incoming
                    .checked_add(bytes)
                    .ok_or("source admission overflow")?;
            }
        }
        let pinned = |owner: Entity, file: usize| {
            self.scenes
                .get(&owner)
                .is_some_and(|s| s.pins.iter().any(|pins| pins.contains(&file)))
                || (owner == scene && plan.sources.contains_key(&file))
        };
        let mut evictable: Vec<_> = self
            .sources
            .iter()
            .filter(|((owner, file), _)| !pinned(*owner, *file))
            .map(|(key, s)| (*key, s.last_used, s.bytes))
            .collect();
        evictable.sort_by_key(|(_, used, _)| *used);
        let required = self
            .statistics
            .resident_bytes
            .checked_add(incoming)
            .ok_or("residency overflow")?;
        let available = evictable.iter().map(|(_, _, bytes)| bytes).sum::<u64>();
        if required.saturating_sub(available) > budget.source_bytes {
            return Err("GPU source budget is pinned by current/pending selections".into());
        }
        let slot_index = self.scenes.get(&scene).map_or(0, |s| s.next);
        let old_capacity = self
            .scenes
            .get(&scene)
            .and_then(|s| s.slots[slot_index].as_ref())
            .map_or(0, |s| s.capacity);
        let capacity = old_capacity.max(plan.padded);
        let other_bytes = self
            .scenes
            .get(&scene)
            .and_then(|s| s.slots[1 - slot_index].as_ref())
            .map_or(0, |s| s.bytes() + scratch_bytes(s.capacity));
        if selected_bytes(capacity) + scratch_bytes(capacity) + other_bytes
            > budget.selected_bytes_per_scene
        {
            return Err("GPU selected slot budget exceeded".into());
        }
        for (key, _, bytes) in evictable {
            if self.statistics.resident_bytes + incoming <= budget.source_bytes {
                break;
            }
            self.sources.remove(&key);
            self.statistics.resident_bytes -= bytes;
        }
        let generation = self.statistics.submitted_generations + 1;
        let upload_timer = (incoming > 0)
            .then(|| cpu.start(crate::diagnostics::sog_cpu::CpuStage::SourceUploadPrepare));
        for (&file, chunk) in &plan.sources {
            let c = chunk.cloud.expanded()?;
            let resident = self.sources.entry((scene, file)).or_insert_with(|| {
                let data = [
                    bytemuck::cast_slice(&c.position_visibility[..chunk.count]),
                    bytemuck::cast_slice(&c.spherical_harmonic[..chunk.count]),
                    bytemuck::cast_slice(&c.rotation[..chunk.count]),
                    bytemuck::cast_slice(&c.scale_opacity[..chunk.count]),
                ];
                let planes = data.map(|contents| {
                    device.create_buffer_with_data(&BufferInitDescriptor {
                        label: Some("SOG immutable source"),
                        contents,
                        usage: BufferUsages::COPY_SRC,
                    })
                });
                let bytes = data.iter().map(|p| p.len() as u64).sum();
                self.statistics.uploaded_bytes += bytes;
                self.statistics.resident_bytes += bytes;
                Source {
                    planes,
                    count: chunk.count,
                    bytes,
                    last_used: generation,
                    bounds: incoming_bounds[&file],
                }
            });
            resident.last_used = generation;
        }
        drop(upload_timer);
        let _copy_timer = cpu.start(crate::diagnostics::sog_cpu::CpuStage::SelectionCopyPrepare);
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for file in plan.sources.keys() {
            let bounds = self.sources[&(scene, *file)].bounds;
            min = min.min(bounds.min().into());
            max = max.max(bounds.max().into());
        }
        let bounds = Aabb::from_min_max(min, max);
        let scene_data = self.scenes.entry(scene).or_default();
        if old_capacity < plan.padded {
            let old = scene_data.slots[slot_index].take();
            let cloud_handle = old
                .as_ref()
                .map_or_else(|| clouds.reserve_handle(), |s| s.cloud_handle.clone());
            let sort_handle = old
                .as_ref()
                .map_or_else(|| sorted.reserve_handle(), |s| s.sort_handle.clone());
            if let Some(old) = old {
                self.statistics.selected_buffer_bytes -= old.bytes();
                self.statistics.radix_scratch_bytes_reserved -= scratch_bytes(old.capacity);
            }
            let [
                position_visibility,
                spherical_harmonic,
                rotation,
                scale_opacity,
            ] = strides().map(|stride| {
                buffer(
                    device,
                    "SOG selected plane",
                    capacity as u64 * stride,
                    BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
                )
            });
            let slot = Slot {
                cloud_handle,
                sort_handle,
                capacity,
                cloud: PlanarStorageGaussian3d {
                    position_visibility,
                    spherical_harmonic,
                    rotation,
                    scale_opacity,
                    count: plan.padded,
                    draw_indirect_buffer: buffer(
                        device,
                        "SOG draw indirect",
                        16,
                        BufferUsages::STORAGE
                            | BufferUsages::INDIRECT
                            | BufferUsages::COPY_DST
                            | BufferUsages::COPY_SRC,
                    ),
                },
                sorted: GpuSortedEntry {
                    sorted_entry_buffer: buffer(
                        device,
                        "SOG sorted entries",
                        capacity as u64 * 8,
                        BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
                    ),
                    count: plan.padded,
                },
            };
            self.statistics.selected_buffer_bytes += slot.bytes();
            self.statistics.selected_slot_capacity += capacity - old_capacity;
            self.statistics.radix_scratch_bytes_reserved += scratch_bytes(capacity);
            self.statistics.allocations += 1;
            scene_data.slots[slot_index] = Some(slot);
        } else {
            self.statistics.reuses += 1;
        }
        let slot = scene_data.slots[slot_index].as_mut().unwrap();
        slot.cloud.count = plan.padded;
        slot.sorted.count = plan.padded;
        let destination = [
            &slot.cloud.position_visibility,
            &slot.cloud.spherical_harmonic,
            &slot.cloud.rotation,
            &slot.cloud.scale_opacity,
        ];
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("SOG selection copies"),
        });
        let mut offset = 0u64;
        for range in plan.ranges.iter().filter(|r| r.count != 0) {
            let src = &self.sources[&(scene, range.file)];
            for (i, stride) in strides().into_iter().enumerate() {
                let bytes = range.count as u64 * stride;
                encoder.copy_buffer_to_buffer(
                    &src.planes[i],
                    range.offset as u64 * stride,
                    destination[i],
                    offset * stride,
                    bytes,
                );
                self.statistics.encoded_copy_bytes += bytes;
            }
            offset += range.count as u64;
        }
        // Clear alignment tail on every reuse; a previously larger selection must
        // never leave opaque splats in the final padded tile.
        for (i, stride) in strides().into_iter().enumerate() {
            let tail = (plan.padded - plan.count) as u64 * stride;
            if tail != 0 {
                encoder.clear_buffer(destination[i], plan.count as u64 * stride, Some(tail));
            }
        }
        encoder.clear_buffer(&slot.cloud.draw_indirect_buffer, 0, None);
        let entity = commands
            .spawn((
                super::readiness::Snapshot,
                super::readiness::GpuSnapshot,
                Visibility::Hidden,
                bevy_gaussian_splatting::PlanarGaussian3dHandle(slot.cloud_handle.clone()),
                bevy_gaussian_splatting::sort::SortedEntriesHandle(slot.sort_handle.clone()),
                bevy_gaussian_splatting::CloudSettings::default(),
                bounds,
                ChildOf(scene),
                Name::new("StreamedSog GPU selection"),
            ))
            .id();
        transfers.0.lock().unwrap().publish.push(Publication {
            selection_copies: WgpuWrapper::new(encoder),
            copy_bytes: plan.count as u64 * strides().iter().sum::<u64>(),
            entity,
            scene,
            cloud_handle: slot.cloud_handle.clone(),
            sort_handle: slot.sort_handle.clone(),
            cloud: slot.cloud.clone(),
            sorted: slot.sorted.clone(),
        });
        scene_data.pins[slot_index] = plan.sources.keys().copied().collect();
        scene_data.next = 1 - slot_index;
        scene_data.selected_count = plan.count;
        self.statistics.selected_count = self.scenes.values().map(|s| s.selected_count).sum();
        self.statistics.submitted_generations = generation;
        Ok(entity)
    }

    /// Once the pending slot is acknowledged, the inactive slot no longer pins
    /// sources. Its selected buffers remain allocated for the next submission.
    pub(super) fn activated(&mut self, scene: Entity) {
        if let Some(s) = self.scenes.get_mut(&scene) {
            s.pins[s.next].clear();
        }
        self.statistics.ready_generations += 1;
    }
}
fn selected_bytes(count: usize) -> u64 {
    count as u64 * (strides().iter().sum::<u64>() + 8) + 16
}
/// Return the radix scratch reservation for this row count.
pub(super) fn scratch_bytes(count: usize) -> u64 {
    let defs = bevy_gaussian_splatting::render::ShaderDefines::default();
    u64::from(defs.sorting_buffer_size)
        + defs.sorting_status_counters_buffer_size(count) as u64
        + count as u64 * 8
        + 16
        + 12 // separate radix indirect dispatch arguments
}
/// Estimate snapshot allocations across the active cameras.
pub(super) fn snapshot_selected_bytes(count: usize, cameras: usize) -> u64 {
    let side = count.isqrt();
    let side = side + usize::from(side * side != count);
    let sort_rows = (side as u64)
        .saturating_mul(side as u64)
        .saturating_mul(cameras.max(1) as u64);
    selected_bytes(count).saturating_add(sort_rows.saturating_sub(count as u64).saturating_mul(8))
}
/// Bound selected rows by the allocation budget and binding limit.
pub(super) fn selected_limit(budget: u64, binding_limit: usize) -> usize {
    // Binary search aligned capacities, accounting exactly for radix tile rounding.
    let mut low = 0;
    let mut high = binding_limit / 32;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if (selected_bytes(mid * 32) + scratch_bytes(mid * 32)) * 2 <= budget {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low * 32
}

/// Bound snapshot rows by per-view allocations and binding capacity.
pub(super) fn snapshot_limit(
    budget: u64,
    binding_limit: usize,
    binding_bytes: u64,
    cameras: usize,
) -> usize {
    // Upstream CPU snapshot sorting pads to a square for every GaussianCamera,
    // whereas generated GPU slots have exactly `count` aligned sort entries.
    let mut low = 0;
    let mut high = binding_limit / 32;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let count = mid * 32;
        let side = count.isqrt();
        let side = side + usize::from(side * side != count);
        let sort_rows = (side as u64)
            .saturating_mul(side as u64)
            .saturating_mul(cameras.max(1) as u64);
        let sort_bytes = sort_rows.saturating_mul(8);
        let pair = snapshot_selected_bytes(count, cameras)
            .saturating_add(scratch_bytes(count))
            .saturating_mul(2);
        if sort_bytes <= binding_bytes && pair <= budget {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low * 32
}

fn buffer(device: &RenderDevice, label: &'static str, size: u64, usage: BufferUsages) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: false,
    })
}

/// Retire allocations whose scene owners no longer exist.
pub fn cleanup(mut gpu: ResMut<GpuResident>, transfers: Res<GpuTransfers>, scenes: Query<Entity>) {
    let removed: Vec<_> = gpu
        .scenes
        .keys()
        .filter(|scene| !scenes.contains(**scene))
        .copied()
        .collect();
    for scene in removed {
        gpu.remove(scene, &transfers);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "gpu_tests.rs"]
mod tests;

#[cfg(test)]
mod scratch_budget_tests {
    use super::*;

    #[test]
    fn snapshot_square_sort_padding_and_camera_count_reduce_capacity() {
        let exact_gpu_pair = (selected_bytes(32) + scratch_bytes(32)) * 2;
        assert_eq!(snapshot_limit(exact_gpu_pair, 32, u64::MAX, 1), 0);
        let snapshot_pair = exact_gpu_pair + 2 * (36 - 32) * 8;
        assert_eq!(snapshot_limit(snapshot_pair, 32, u64::MAX, 1), 32);
        assert_eq!(snapshot_limit(snapshot_pair, 32, u64::MAX, 2), 0);
        assert_eq!(snapshot_limit(u64::MAX, 32, 36 * 8 - 1, 1), 0);
    }
    #[test]
    fn both_slots_reserve_indirect_dispatch_before_admission() {
        let capacity = 32;
        let defs = bevy_gaussian_splatting::render::ShaderDefines::default();
        let old_scratch = u64::from(defs.sorting_buffer_size)
            + defs.sorting_status_counters_buffer_size(capacity) as u64
            + capacity as u64 * 8
            + 16;
        let old_budget = (selected_bytes(capacity) + old_scratch) * 2;
        assert_eq!(selected_limit(old_budget, capacity), 0);
        assert_eq!(selected_limit(old_budget + 24, capacity), capacity);
    }
}
