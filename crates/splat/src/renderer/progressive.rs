//! Fixed-capacity GPU ownership for one progressive PLY generation.
//!
//! New rows are staged independently and submitted at synchronized extraction.
//! Unreceived rows stay zero/invisible. Radix sorting retains the full declared
//! padded capacity throughout loading, including its scratch reservation.
use super::{
    gpu::{self, GpuBudget, GpuTransfers, Publication},
    readiness::{GpuSnapshot, Snapshot},
};
use bevy::{
    camera::primitives::Aabb,
    prelude::*,
    render::{
        render_resource::*,
        renderer::{RenderDevice, WgpuWrapper},
    },
};
use bevy_gaussian_splatting::{
    CloudSettings, GaussianMode, GaussianRasterProfile, PlanarGaussian3d, PlanarGaussian3dHandle,
    gaussian::formats::planar_3d::PlanarStorageGaussian3d,
    sort::{GpuSortedEntry, SortedEntries, SortedEntriesHandle},
};

/// GPU-owned cloud without a retained CPU asset for comparison sorting.
#[derive(Component)]
pub(crate) struct ProgressiveCloud;

pub(crate) struct ProgressiveGpu {
    entity: Entity,
    total: usize,
    received: usize,
    uploaded_bytes: u64,
    cloud_handle: Handle<PlanarGaussian3d>,
    sort_handle: Handle<SortedEntries>,
    cloud: PlanarStorageGaussian3d,
    sorted: GpuSortedEntry,
    bounds: Option<(Vec3, Vec3)>,
    support: f32,
}

impl ProgressiveGpu {
    pub(crate) fn new(world: &mut World, total: usize) -> Result<Self, String> {
        let device = world
            .get_resource::<RenderDevice>()
            .ok_or("progressive PLY requires a GPU render device")?;
        let limits = device.limits();
        let capacity = checked_capacity(
            total,
            limits.max_storage_buffer_binding_size,
            limits.max_buffer_size,
        )?;
        let budget = world.get_resource::<GpuBudget>().map_or_else(
            || GpuBudget::default().selected_bytes_per_scene,
            |b| b.selected_bytes_per_scene,
        );
        let bytes = capacity as u64 * (gpu::strides().iter().sum::<u64>() + 8)
            + 16
            + gpu::scratch_bytes(capacity);
        if bytes > budget {
            return Err("progressive PLY capacity exceeds GPU selection budget".into());
        }
        if !world.contains_resource::<GpuTransfers>()
            || !world.contains_resource::<Assets<PlanarGaussian3d>>()
            || !world.contains_resource::<Assets<SortedEntries>>()
        {
            return Err("progressive PLY renderer resources are unavailable".into());
        }
        let [
            position_visibility,
            spherical_harmonic,
            rotation,
            scale_opacity,
        ] = gpu::strides().map(|stride| {
            buffer(
                device,
                "progressive PLY plane",
                capacity as u64 * stride,
                BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            )
        });
        let cloud = PlanarStorageGaussian3d {
            position_visibility,
            spherical_harmonic,
            rotation,
            scale_opacity,
            count: capacity,
            draw_indirect_buffer: buffer(
                device,
                "progressive PLY indirect draw",
                16,
                BufferUsages::STORAGE
                    | BufferUsages::INDIRECT
                    | BufferUsages::COPY_DST
                    | BufferUsages::COPY_SRC,
            ),
        };
        let sorted = GpuSortedEntry {
            sorted_entry_buffer: buffer(
                device,
                "progressive PLY sorted entries",
                capacity as u64 * 8,
                BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            ),
            count: capacity,
        };
        let cloud_handle = world
            .resource::<Assets<PlanarGaussian3d>>()
            .reserve_handle();
        let sort_handle = world.resource::<Assets<SortedEntries>>().reserve_handle();
        let entity = world
            .spawn((
                ProgressiveCloud,
                Snapshot,
                GpuSnapshot,
                PlanarGaussian3dHandle(cloud_handle.clone()),
                SortedEntriesHandle(sort_handle.clone()),
                CloudSettings {
                    gaussian_mode: GaussianMode::Gaussian3d,
                    raster_profile: GaussianRasterProfile::NormalizedGaussian,
                    ..default()
                },
                Transform::default(),
                Aabb::from_min_max(Vec3::ZERO, Vec3::ZERO),
                Visibility::Hidden,
                Name::new("Progressive PLY"),
            ))
            .id();
        Ok(Self {
            entity,
            total,
            received: 0,
            uploaded_bytes: 0,
            cloud_handle,
            sort_handle,
            cloud,
            sorted,
            bounds: None,
            support: 0.,
        })
    }

    pub(crate) fn append(
        &mut self,
        world: &mut World,
        batch: &crate::progressive::decoder::Batch,
    ) -> Result<(), String> {
        let end = self
            .received
            .checked_add(batch.count)
            .ok_or("progressive PLY row count overflow")?;
        if batch.count == 0 || batch.total != self.total || end > self.total {
            return Err("progressive PLY batch exceeds declared count".into());
        }
        if [
            batch.cloud.position_visibility.len(),
            batch.cloud.spherical_harmonic.len(),
            batch.cloud.rotation.len(),
            batch.cloud.scale_opacity.len(),
        ]
        .into_iter()
        .any(|len| len != batch.count)
        {
            return Err("progressive PLY batch planes have inconsistent counts".into());
        }
        let (min, max) = batch.bounds;
        if !min.is_finite()
            || !max.is_finite()
            || !min.cmple(max).all()
            || !batch.support.is_finite()
            || batch.support < 0.
        {
            return Err("progressive PLY batch bounds are invalid".into());
        }
        let support = self.support.max(batch.support);
        let (min, max) = self
            .bounds
            .map_or((min, max), |old| (old.0.min(min), old.1.max(max)));
        let bounds = Aabb::from_min_max(min - Vec3::splat(support), max + Vec3::splat(support));
        if !bounds.center.is_finite() || !bounds.half_extents.is_finite() {
            return Err("progressive PLY batch bounds overflow".into());
        }
        if world.get_entity(self.entity).is_err() {
            return Err("progressive PLY entity was removed".into());
        }
        let device = world
            .get_resource::<RenderDevice>()
            .ok_or("progressive PLY requires a GPU render device")?;
        let transfers = world
            .get_resource::<GpuTransfers>()
            .ok_or("progressive PLY transfer queue is unavailable")?;
        let planes = [
            bytemuck::cast_slice(batch.cloud.position_visibility.as_slice()),
            bytemuck::cast_slice(batch.cloud.spherical_harmonic.as_slice()),
            bytemuck::cast_slice(batch.cloud.rotation.as_slice()),
            bytemuck::cast_slice(batch.cloud.scale_opacity.as_slice()),
        ];
        let destination = [
            &self.cloud.position_visibility,
            &self.cloud.spherical_harmonic,
            &self.cloud.rotation,
            &self.cloud.scale_opacity,
        ];
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("progressive PLY append copies"),
        });
        for ((contents, destination), stride) in
            planes.into_iter().zip(destination).zip(gpu::strides())
        {
            // The encoder retains each staging buffer until its copy submits.
            // No decoded rows or full CPU cloud survive this operation.
            let staging = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("progressive PLY append staging"),
                contents,
                usage: BufferUsages::COPY_SRC,
            });
            encoder.copy_buffer_to_buffer(
                &staging,
                0,
                destination,
                self.received as u64 * stride,
                contents.len() as u64,
            );
        }
        let copy_bytes = batch.count as u64 * gpu::strides().iter().sum::<u64>();
        transfers.0.lock().unwrap().publish.push(Publication {
            selection_copies: WgpuWrapper::new(encoder),
            copy_bytes,
            entity: self.entity,
            scene: self.entity,
            cloud_handle: self.cloud_handle.clone(),
            sort_handle: self.sort_handle.clone(),
            cloud: self.cloud.clone(),
            sorted: self.sorted.clone(),
        });
        world.entity_mut(self.entity).insert(bounds);
        self.received = end;
        self.uploaded_bytes += copy_bytes;
        self.bounds = Some((min, max));
        self.support = support;
        Ok(())
    }

    pub(crate) fn entity(&self) -> Entity {
        self.entity
    }

    /// Bytes staged/encoded, including copies canceled before extraction.
    #[cfg(test)]
    pub(crate) fn uploaded_bytes(&self) -> u64 {
        self.uploaded_bytes
    }

    pub(crate) fn retire(self, world: &mut World) {
        if let Some(transfers) = world.get_resource::<GpuTransfers>() {
            let mut transfers = transfers.0.lock().unwrap();
            transfers.publish.retain(|p| p.entity != self.entity);
            transfers
                .retire
                .push((self.cloud_handle.id(), self.sort_handle.id()));
        }
        if let Some(ready) = world.get_resource::<super::readiness::ReadySnapshots>() {
            ready.0.lock().unwrap().remove(&self.entity);
        }
        world.despawn(self.entity);
    }
}

fn checked_capacity(total: usize, binding: u64, maximum: u64) -> Result<usize, String> {
    let capacity = total
        .checked_add(31)
        .map(|n| n / 32 * 32)
        .filter(|n| *n > 0 && *n <= u32::MAX as usize)
        .ok_or("progressive PLY declared count is invalid")?;
    let defs = bevy_gaussian_splatting::render::ShaderDefines::default();
    let mut sizes = gpu::strides()
        .map(|stride| (capacity as u64).checked_mul(stride))
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("progressive PLY buffer size overflow")?;
    // Calculate status-counter bytes in u64 before upstream's usize arithmetic;
    // an untrusted declared count must also fail safely on 32-bit WASM.
    let status_bytes = (capacity as u64).div_ceil(u64::from(defs.workgroup_entries_c))
        * u64::from(defs.radix_base)
        * 4;
    if status_bytes > usize::MAX as u64 {
        return Err("progressive PLY radix size overflow".into());
    }
    sizes.extend([
        capacity as u64 * 8,
        u64::from(defs.sorting_buffer_size),
        status_bytes,
        16,
    ]);
    if sizes
        .into_iter()
        .any(|size| size > binding || size > maximum)
        || maximum < 16
    {
        return Err("progressive PLY capacity exceeds GPU buffer limits".into());
    }
    Ok(capacity)
}

fn buffer(device: &RenderDevice, label: &'static str, size: u64, usage: BufferUsages) -> Buffer {
    // WebGPU initializes newly allocated contents to zero before observation.
    device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: false,
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "progressive_gpu_tests.rs"]
mod tests;
