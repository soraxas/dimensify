//! GPU-only, per-view compact clouds. The producer owns reservation/publication;
//! immutable source ranges can be baked automatically before sorting. No
//! expanded CPU or GPU cloud is created.
use bevy::{
    prelude::*,
    render::{render_resource::*, renderer::RenderDevice},
};

/// Three integer working planes plus indirect draw metadata. A colour plane is
/// valid for exactly one view: sharing it between cameras would reuse stale SH.
/// Attach with a unique reserved `PlanarGaussian3dHandle`, `SortedEntriesHandle`,
/// explicit bounds and supported `CloudSettings`. The reserved cloud handle is
/// only an allocation identity: never insert an expanded asset under that handle.
/// Replace the entity/identity when switching between expanded and compact paths.
#[derive(Component, Clone)]
pub struct CompactCloud {
    planes: [Buffer; 3],
    indirect: Buffer,
    count: u32,
    view: Entity,
    pub(crate) ready: bool,
    pub(crate) pipeline_ready: bool,
    draw_enabled: bool,
    pub(crate) projected: Option<super::projected::ProjectionCache>,
    #[cfg(feature = "io_sog")]
    pub(super) sources: Option<std::sync::Arc<super::sog::view::Sources>>,
}

impl CompactCloud {
    /// Buffers must have STORAGE usage; indirect also needs INDIRECT/COPY_DST.
    /// `view` is the target camera's **render-world** entity. The owner must
    /// finish writes before publishing and never mutate active buffers from a
    /// concurrently running main-world update.
    pub fn new(
        planes: [Buffer; 3],
        indirect: Buffer,
        count: u32,
        view: Entity,
    ) -> Result<Self, &'static str> {
        if !supported_build() {
            return Err(
                "compact work requires planar storage and radix without morph/precomputed covariance",
            );
        }
        if count == 0
            || planes.iter().zip([16u64, 4, 4]).any(|(b, stride)| {
                b.size() < u64::from(count) * stride || !b.usage().contains(BufferUsages::STORAGE)
            })
        {
            return Err("compact planes do not cover the logical selection");
        }
        if indirect.size() < 16
            || !indirect
                .usage()
                .contains(BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST)
        {
            return Err("compact indirect buffer lacks capacity or required usages");
        }
        Ok(Self {
            planes,
            indirect,
            count,
            view,
            ready: true,
            pipeline_ready: false,
            draw_enabled: true,
            projected: None,
            #[cfg(feature = "io_sog")]
            sources: None,
        })
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn view(&self) -> Entity {
        self.view
    }
    /// Prepare hidden replacement generations without drawing them over current coverage.
    pub fn set_draw_enabled(&mut self, enabled: bool) {
        self.draw_enabled = enabled;
    }
    pub fn draw_enabled(&self) -> bool {
        self.draw_enabled
    }
    /// Render-world readiness, not GPU completion or a main-world prediction.
    pub fn is_prepared(&self) -> bool {
        self.ready && self.pipeline_ready && self.projection_ready()
    }
    /// Reserve 32 bytes per logical candidate, in addition to source/work/sort
    /// memory. The caller must include this in transition admission. Only the
    /// normalized 3D colour/OBB path is currently supported. The projection
    /// schedule is installed by the `io_sog` integration.
    #[cfg(feature = "io_sog")]
    pub fn enable_projection_cache(
        &mut self,
        device: &RenderDevice,
        allowance: u64,
    ) -> Result<(), &'static str> {
        let size = u64::from(self.count) * super::projected::RECORD_BYTES;
        if size > allowance
            || size > device.limits().max_storage_buffer_binding_size as u64
            || size > device.limits().max_buffer_size
        {
            return Err("projection cache exceeds allowance/device binding");
        }
        self.set_projection_cache(device.create_buffer(&BufferDescriptor {
            label: Some("SOG projected records"),
            size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }))
    }
    /// Attach an already-admitted slot allocation. Callers must retain it until
    /// the previous render generation has retired before reusing the slot.
    #[cfg(feature = "io_sog")]
    pub fn set_projection_cache(&mut self, buffer: Buffer) -> Result<(), &'static str> {
        if buffer.size() < u64::from(self.count) * super::projected::RECORD_BYTES
            || !buffer.usage().contains(BufferUsages::STORAGE)
        {
            return Err("projection cache is too small or lacks storage usage");
        }
        self.projected = Some(super::projected::ProjectionCache {
            buffer,
            ready: false,
        });
        self.pipeline_ready = false;
        Ok(())
    }
    pub fn has_projection_cache(&self) -> bool {
        self.projected.is_some()
    }
    pub(crate) fn projection_ready(&self) -> bool {
        self.projected.as_ref().is_none_or(|p| p.ready)
    }
    pub(crate) fn same_generation(&self, other: &Self) -> bool {
        #[cfg(feature = "io_sog")]
        if !match (&self.sources, &other.sources) {
            (None, None) => true,
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        } {
            return false;
        }
        self.count == other.count
            && self.projected.as_ref().map(|p| p.buffer.id())
                == other.projected.as_ref().map(|p| p.buffer.id())
            && self.view == other.view
            && self.indirect.id() == other.indirect.id()
            && self
                .planes
                .iter()
                .zip(&other.planes)
                .all(|(a, b)| a.id() == b.id())
    }
    /// Borrow the actual working allocations shared with the producer. These
    /// handles preserve BufferIds across main/render-world ownership handoffs.
    pub fn storage_buffers(&self) -> [&Buffer; 3] {
        self.planes.each_ref()
    }
    /// Borrow the optional projection allocation without retaining its owner.
    pub fn projection_buffer(&self) -> Option<&Buffer> {
        self.projected.as_ref().map(|projection| &projection.buffer)
    }
    pub fn draw_indirect_buffer(&self) -> &Buffer {
        &self.indirect
    }
    pub fn bind_group(&self, device: &RenderDevice, layout: &BindGroupLayout) -> BindGroup {
        if let Some(projected) = &self.projected {
            return device.create_bind_group(
                "projected SOG work",
                layout,
                &[BindGroupEntry {
                    binding: 0,
                    resource: projected.buffer.as_entire_binding(),
                }],
            );
        }
        self.work_bind_group(device, layout)
    }
    pub(crate) fn work_bind_group(
        &self,
        device: &RenderDevice,
        layout: &BindGroupLayout,
    ) -> BindGroup {
        device.create_bind_group(
            "compact SOG work",
            layout,
            &self
                .planes
                .each_ref()
                .map(|b| b.as_entire_binding())
                .into_iter()
                .enumerate()
                .map(|(i, resource)| BindGroupEntry {
                    binding: i as u32,
                    resource,
                })
                .collect::<Vec<_>>(),
        )
    }
}

fn supported_build() -> bool {
    cfg!(all(
        feature = "planar",
        feature = "buffer_storage",
        feature = "sort_radix",
        not(feature = "buffer_texture"),
        not(feature = "packed"),
        not(feature = "morph_particles"),
        not(feature = "precompute_covariance_3d")
    ))
}

/// GPU-only inputs cannot use CPU sorting or 4D storage. Callers should validate
/// this before publication; queueing rejects incompatible settings too.
pub fn supports(settings: &crate::CloudSettings) -> bool {
    #[cfg(all(feature = "sort_radix", not(feature = "buffer_texture")))]
    {
        supported_build()
            && settings.sort_mode == crate::sort::SortMode::Radix
            && settings.gaussian_mode == crate::gaussian::settings::GaussianMode::Gaussian3d
    }
    #[cfg(not(all(feature = "sort_radix", not(feature = "buffer_texture"))))]
    {
        let _ = settings;
        false
    }
}

/// Radix reads only centres. Keeping the other planes out of compute visibility
/// preserves the baseline eight-storage-buffer limit alongside radix scratch.
pub fn layout_descriptor(radix: bool) -> BindGroupLayoutDescriptor {
    let entries: Vec<_> = [16, 4, 4]
        .into_iter()
        .enumerate()
        .map(|(binding, size)| BindGroupLayoutEntry {
            binding: binding as u32,
            visibility: if radix && binding != 0 {
                ShaderStages::VERTEX
            } else {
                ShaderStages::VERTEX | ShaderStages::COMPUTE
            },
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: BufferSize::new(size),
            },
            count: None,
        })
        .collect();
    BindGroupLayoutDescriptor::new("compact SOG work", &entries)
}
