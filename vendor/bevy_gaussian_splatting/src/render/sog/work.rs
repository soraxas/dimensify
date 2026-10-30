//! Range-addressed, 24-byte/splat working storage. Geometry is view-independent;
//! only the packed colour plane needs refreshing for higher-order SH on motion.
//! These kernels do not publish generations, sort or draw. The renderer owner
//! must order them before consumers and retain the previous ready generation.

use super::{DECODE_SHADER, GpuSogSource, diagnostics::BakeDiagnostics};

mod batch;

pub const WORK_SHADER: &str = include_str!("work.wgsl");

#[derive(Clone, Copy, Debug)]
pub struct BakeRange {
    pub source_start: u32,
    pub count: u32,
    pub destination_start: u32,
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    use bevy::math::Vec3;

    #[test]
    fn color_threshold_scales_with_distance_but_not_perspective_look_direction() {
        let bounds = [Vec3::splat(-1.), Vec3::splat(1.)];
        let ratio = 10_f32.to_radians().tan();
        let inside = LocalView::perspective([0., 0., 0.]).unwrap();
        let far = LocalView::perspective([0., 0., 11.]).unwrap();
        assert_eq!(inside.color_refresh_distance(bounds, ratio), ratio);
        assert_eq!(far.color_refresh_distance(bounds, ratio), ratio * 10.);
        assert_eq!(far.translation_since(far), Some(0.));
        assert_eq!(far.translation_since(inside), Some(11.));
        let ortho_x = LocalView::orthographic([1., 0., 0.]).unwrap();
        let ortho_y = LocalView::orthographic([0., 1., 0.]).unwrap();
        assert_eq!(ortho_x.translation_since(far), None);
        assert_eq!(far.translation_since(ortho_x), None);
        assert_eq!(ortho_x.translation_since(ortho_y), None);
        assert_eq!(ortho_x.translation_since(ortho_x), Some(0.));
    }
}

impl BakeRange {
    pub fn validate(
        self,
        source_count: u32,
        destination_capacity: u32,
    ) -> Result<(), &'static str> {
        let source_end = self
            .source_start
            .checked_add(self.count)
            .ok_or("source range overflow")?;
        let destination_end = self
            .destination_start
            .checked_add(self.count)
            .ok_or("destination range overflow")?;
        if source_end > source_count || destination_end > destination_capacity {
            return Err("bake range exceeds logical source count or destination capacity");
        }
        Ok(())
    }
}

/// A camera-to-splat direction expressed in the source's coordinate frame.
/// Consumers apply the inverse scene transform before constructing this value.
/// Orthographic views use parallel rays rather than camera-to-centre vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalView {
    position: [f32; 4],
    direction: [f32; 4],
}

impl LocalView {
    /// Translation in source space. A projection-mode or orthographic direction
    /// change cannot reuse perspective SH and requests an exact refresh.
    pub(super) fn translation_since(self, old: Self) -> Option<f32> {
        if self == old {
            return Some(0.0);
        }
        if self.position[3] != 1.0 || old.position[3] != 1.0 {
            return None;
        }
        Some(
            bevy::math::Vec3::from_slice(&self.position)
                .distance(bevy::math::Vec3::from_slice(&old.position)),
        )
    }

    pub(super) fn color_refresh_distance(self, bounds: [bevy::math::Vec3; 2], ratio: f32) -> f32 {
        let eye = bevy::math::Vec3::from_slice(&self.position);
        // Reference-viewer policy: near-field floor of one source-space unit.
        // This controls refresh frequency, not a rigorous colour-error bound.
        eye.distance(eye.clamp(bounds[0], bounds[1])).max(1.0) * ratio
    }

    pub fn perspective(position: [f32; 3]) -> Result<Self, &'static str> {
        if position.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite local camera position");
        }
        Ok(Self {
            position: [position[0], position[1], position[2], 1.0],
            direction: [0.0; 4],
        })
    }

    pub fn orthographic(direction: [f32; 3]) -> Result<Self, &'static str> {
        let d = bevy::math::Vec3::from_array(direction)
            .try_normalize()
            .ok_or("orthographic direction must be finite and nonzero")?;
        Ok(Self {
            position: [0.0; 4],
            direction: [d.x, d.y, d.z, 0.0],
        })
    }
}

/// Three tightly packed planes: centre/scale/opacity (16), rotation (4), RGB (4).
/// No per-splat coefficient array. Counts are logical, not square-texture padded.
pub struct CompactWork {
    transform: bevy::render::render_resource::Buffer,
    rotation: bevy::render::render_resource::Buffer,
    color: bevy::render::render_resource::Buffer,
    capacity: u32,
}

impl CompactWork {
    pub fn required_bytes(capacity: u32) -> u64 {
        u64::from(capacity) * 24
    }

    /// Allocate only against a caller-reserved payload allowance. The caller
    /// additionally accounts for old/new generations, sort scratch and sources.
    pub fn new(device: &wgpu::Device, capacity: u32, budget: u64) -> Result<Self, &'static str> {
        if capacity == 0 {
            return Err("empty work buffer allocation");
        }
        if Self::required_bytes(capacity) > budget {
            return Err("compact work budget exceeded");
        }
        let limits = device.limits();
        let max_plane = u64::from(capacity) * 16;
        if max_plane > limits.max_buffer_size
            || max_plane > u64::from(limits.max_storage_buffer_binding_size)
        {
            return Err("compact work plane exceeds device buffer limit");
        }
        let allocate = |stride| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("compact SOG work plane"),
                size: u64::from(capacity) * stride,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Ok(Self {
            transform: allocate(16).into(),
            rotation: allocate(4).into(),
            color: allocate(4).into(),
            capacity,
        })
    }

    pub fn buffers(&self) -> [&wgpu::Buffer; 3] {
        [&self.transform, &self.rotation, &self.color]
    }
    /// Stable allocation identities shared with compact render generations.
    /// Borrowing these handles does not extend their ownership lifetime.
    pub fn storage_buffers(&self) -> [&bevy::render::render_resource::Buffer; 3] {
        [&self.transform, &self.rotation, &self.color]
    }
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

/// Generic range decoder. All writes are encoded on the caller's command
/// encoder; no hidden queue submit or CPU readback occurs in the production API.
pub struct CompactBaker {
    pipeline: wgpu::ComputePipeline,
    work_layout: wgpu::BindGroupLayout,
    dynamic_uniforms: bool,
}

impl CompactBaker {
    pub fn new(device: &wgpu::Device) -> Result<Self, String> {
        let limits = device.limits();
        let dynamic_uniforms = limits.max_dynamic_uniform_buffers_per_pipeline_layout > 0;
        if limits.max_bind_groups < 2
            || limits.max_storage_buffers_per_shader_stage < 3
            || limits.max_sampled_textures_per_shader_stage < 8
            || limits.max_uniform_buffers_per_shader_stage < 2
            || limits.max_bindings_per_bind_group < 9
            || limits.max_compute_invocations_per_workgroup < 64
            || limits.max_compute_workgroup_size_x < 64
        {
            return Err("device cannot run the compact SOG compute kernel".into());
        }
        let source = format!("{DECODE_SHADER}\n{WORK_SHADER}");
        let module =
            wgpu::naga::front::wgsl::parse_str(&source).map_err(|e| e.emit_to_string(&source))?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compact SOG range decoder"),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
        });
        let mut entries = (0..3)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect::<Vec<_>>();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic_uniforms,
                min_binding_size: std::num::NonZeroU64::new(48),
            },
            count: None,
        });
        let work_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compact SOG work layout"),
            entries: &entries,
        });
        let source_layout = GpuSogSource::layout(device);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&source_layout), Some(&work_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compact SOG range decoder"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("sog_bake"),
            compilation_options: Default::default(),
            cache: None,
        });
        Ok(Self {
            pipeline,
            work_layout,
            dynamic_uniforms,
        })
    }

    /// Encode one checked source-to-destination range. In-place colour refresh is
    /// permitted only when the renderer has exclusive write access for this frame.
    /// Each dispatch owns immutable parameters so deferred submissions cannot
    /// accidentally observe the last range/view written to a shared uniform.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &GpuSogSource,
        work: &CompactWork,
        range: BakeRange,
        view: LocalView,
        color_only: bool,
    ) -> Result<(), &'static str> {
        self.encode_ranges_observed(
            device,
            encoder,
            work,
            std::iter::once((source, range)),
            view,
            color_only,
            &BakeDiagnostics::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn encode_ranges_observed<'a>(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        work: &CompactWork,
        ranges: impl IntoIterator<Item = (&'a GpuSogSource, BakeRange)>,
        view: LocalView,
        color_only: bool,
        diagnostics: &BakeDiagnostics,
    ) -> Result<(), &'static str> {
        batch::encode(
            self,
            device,
            encoder,
            work,
            ranges,
            view,
            color_only,
            diagnostics,
        )
    }
}
