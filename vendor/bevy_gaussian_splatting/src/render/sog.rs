//! Generic immutable SOG data-texture bindings. Streaming policy lives in the app.
//!
//! This is an opt-in building block, not a replacement for `PlanarGaussian3d`.
//! Uploads retain no CPU attributes or expanded SH arrays. Callers own source
//! lifetime, admission, device-error handling and submission/readiness tracking.

use std::sync::Arc;

use wgpu::util::DeviceExt;

use crate::io::sog_packed::PackedSog;

/// Portable decoder functions; consuming compute shaders supply their entry point.
pub const DECODE_SHADER: &str = include_str!("sog_decode.wgsl");

pub mod diagnostics;
pub mod view;
pub mod work;

/// Errors detectable before creating GPU resources. Device loss/OOM remain the
/// render owner's responsibility; these checks do not promise physical VRAM.
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("packed SOG requires {required} bytes, admitted budget is {budget}")]
    Budget { required: u64, budget: u64 },
    #[error("packed SOG textures exceed device dimension limit {0}")]
    Dimensions(u32),
    #[error("packed SOG requires eight sampled textures and one uniform binding")]
    Bindings,
    #[error("shared SOG palette metadata differs from the source")]
    Palette,
}

/// Immutable per-file palette and lookup table shared by range pages. Contains
/// GPU handles and small lookup metadata only, never CPU texture storage.
/// The caller owns the original-file identity: matching metadata alone does not
/// establish equality of two independently authored centroid textures.
pub struct GpuSogPalette {
    centroids: wgpu::TextureView,
    lut: wgpu::TextureView,
    books: [[f32; 256]; 3],
    dimensions: [u32; 2],
    encoded_bands: u32,
    palette_count: u32,
    resident_bytes: u64,
}

impl GpuSogPalette {
    /// Charge once while any page or cache entry owns this palette.
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    fn compatible(&self, source: &PackedSog) -> bool {
        let higher = source.higher_order();
        self.books[0] == *source.scale_codebook()
            && self.books[1] == *source.sh0_codebook()
            && self.books[2] == higher.map_or([0.0; 256], |h| *h.codebook())
            && self.dimensions == higher.map_or([1, 1], |h| h.centroids().dimensions())
            && self.encoded_bands == higher.map_or(0, |h| h.encoded_bands())
            && self.palette_count == higher.map_or(0, |h| h.palette_count())
    }
}

/// Immutable source bindings. The bind group retains its textures and uniform;
/// cloning the CPU source is not necessary after upload. Dropping this handle
/// leaves submitted GPU commands' resource lifetimes managed by wgpu.
pub struct GpuSogSource {
    bind_group: wgpu::BindGroup,
    count: u32,
    bands: u32,
    resident_bytes: u64,
    palette: Arc<GpuSogPalette>,
    /// Conservative source-space centre bounds; no CPU texels are retained.
    center_bounds: [bevy::math::Vec3; 2],
}

impl GpuSogSource {
    /// Requested payload, including LUT, uniform and SH0 fallback textures.
    /// Excludes driver allocation granularity and transient upload staging.
    pub fn required_bytes(source: &PackedSog) -> u64 {
        source.texture_bytes()
            + 256 * 16
            + 48
            + if source.higher_order().is_none() {
                8
            } else {
                0
            }
    }

    /// Layout shared by every source. No filtering, sRGB conversion or f16 is used.
    pub fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        let mut entries = (0..8)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: if binding == 7 {
                        wgpu::TextureSampleType::Float { filterable: false }
                    } else {
                        wgpu::TextureSampleType::Uint
                    },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect::<Vec<_>>();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: std::num::NonZeroU64::new(48),
            },
            count: None,
        });
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("packed SOG source layout"),
            entries: &entries,
        })
    }

    /// Upload within an already-reserved payload budget. `layout` must come from
    /// `Self::layout` on this device. All validation precedes GPU allocation.
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        source: &PackedSog,
        budget: u64,
    ) -> Result<Self, UploadError> {
        let mut upload = Self::begin_upload(device, queue, layout, source, budget)?;
        upload.write(queue, source, usize::MAX);
        Ok(upload.finish().ok().expect("unbounded upload completes"))
    }

    /// Reserve the entire immutable source, then feed data with bounded `write`
    /// calls. No partial source can be bound by the renderer. The caller keeps
    /// the same CPU source alive until completion (this object retains no texels).
    pub fn begin_upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        source: &PackedSog,
        budget: u64,
    ) -> Result<GpuSogUpload, UploadError> {
        Self::begin_upload_inner(device, queue, layout, source, None, budget)
    }

    /// Upload a page using the completed palette of another page from the same
    /// immutable original file. `budget` admits only this page's exclusive
    /// planes/uniform; the caller has already charged `palette.resident_bytes()`.
    /// Metadata compatibility is checked, but original-file/content identity is
    /// the caller's responsibility. No palette texels or LUT are copied again.
    pub fn begin_upload_with_palette(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        source: &PackedSog,
        palette: &Arc<GpuSogPalette>,
        budget: u64,
    ) -> Result<GpuSogUpload, UploadError> {
        Self::begin_upload_inner(device, queue, layout, source, Some(palette.clone()), budget)
    }

    fn begin_upload_inner(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        source: &PackedSog,
        shared: Option<Arc<GpuSogPalette>>,
        budget: u64,
    ) -> Result<GpuSogUpload, UploadError> {
        if shared
            .as_ref()
            .is_some_and(|palette| !palette.compatible(source))
        {
            return Err(UploadError::Palette);
        }
        let required = Self::required_bytes(source)
            - shared
                .as_ref()
                .map_or(0, |palette| palette.resident_bytes());
        if required > budget {
            return Err(UploadError::Budget { required, budget });
        }
        let limits = device.limits();
        if limits.max_sampled_textures_per_shader_stage < 8
            || limits.max_uniform_buffers_per_shader_stage < 1
            || limits.max_bindings_per_bind_group < 9
        {
            return Err(UploadError::Bindings);
        }
        let higher = source.higher_order();
        let textures = [
            Some(source.means_lo()),
            Some(source.means_hi()),
            Some(source.quats()),
            Some(source.scales()),
            Some(source.sh0()),
            higher.map(|h| h.labels()),
            higher.map(|h| h.centroids()),
        ];
        if limits.max_texture_dimension_2d < 256
            || textures.iter().flatten().any(|t| {
                t.dimensions()
                    .iter()
                    .any(|d| *d > limits.max_texture_dimension_2d)
            })
        {
            return Err(UploadError::Dimensions(limits.max_texture_dimension_2d));
        }
        let textures = textures
            .into_iter()
            .take(if shared.is_some() { 6 } else { 7 })
            .map(|data| {
                create_texture(
                    device,
                    data.map_or([1, 1], |t| t.dimensions()),
                    wgpu::TextureFormat::Rgba8Uint,
                )
            })
            .collect::<Vec<_>>();
        let mut views = textures
            .iter()
            .map(|texture| texture.create_view(&Default::default()))
            .collect::<Vec<_>>();
        let palette = if let Some(palette) = shared {
            views.push(palette.centroids.clone());
            palette
        } else {
            let mut lut = [[0.0f32; 4]; 256];
            for (i, entry) in lut.iter_mut().enumerate() {
                *entry = [
                    source.scale_codebook()[i],
                    source.sh0_codebook()[i],
                    higher.map_or(0.0, |h| h.codebook()[i]),
                    0.0,
                ];
            }
            Arc::new(GpuSogPalette {
                centroids: views[6].clone(),
                lut: upload_texture(
                    device,
                    queue,
                    [256, 1],
                    wgpu::TextureFormat::Rgba32Float,
                    bytemuck::cast_slice(&lut),
                    16,
                ),
                books: [
                    *source.scale_codebook(),
                    *source.sh0_codebook(),
                    higher.map_or([0.0; 256], |h| *h.codebook()),
                ],
                dimensions: higher.map_or([1, 1], |h| h.centroids().dimensions()),
                encoded_bands: higher.map_or(0, |h| h.encoded_bands()),
                palette_count: higher.map_or(0, |h| h.palette_count()),
                resident_bytes: 4096 + higher.map_or(4, |h| h.centroids().byte_len()),
            })
        };
        views.push(palette.lut.clone());
        let min = source.means_min();
        let max = source.means_max();
        let geometry = [min[0], min[1], min[2], 0.0, max[0], max[1], max[2], 0.0];
        let bands = higher.map_or(0, |h| h.bands());
        let encoded_coefficients = higher.map_or(0, |h| match h.encoded_bands() {
            1 => 3,
            2 => 8,
            3 => 15,
            _ => 0,
        });
        let counts = [
            source.count(),
            source.means_lo().dimensions()[0],
            bands,
            encoded_coefficients,
        ];
        let mut uniform_bytes = Vec::with_capacity(48);
        uniform_bytes.extend_from_slice(bytemuck::cast_slice(&geometry));
        uniform_bytes.extend_from_slice(bytemuck::cast_slice(&counts));
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("packed SOG metadata"),
            contents: &uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut entries = views
            .iter()
            .enumerate()
            .map(|(i, view)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: wgpu::BindingResource::TextureView(view),
            })
            .collect::<Vec<_>>();
        entries.push(wgpu::BindGroupEntry {
            binding: 8,
            resource: uniform.as_entire_binding(),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("packed SOG immutable source"),
            layout,
            entries: &entries,
        });
        Ok(GpuSogUpload {
            source: Self {
                bind_group,
                count: source.count(),
                bands,
                resident_bytes: Self::required_bytes(source),
                palette,
                center_bounds: [min, max].map(|point| {
                    bevy::math::Vec3::from_array(point.map(|v| v.signum() * v.abs().exp_m1()))
                }),
            },
            allocated_bytes: required,
            textures,
            plane: 0,
            row: 0,
        })
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn bands(&self) -> u32 {
        self.bands
    }
    /// Referenced payload, including the shared palette. For a multi-page cache,
    /// sum `exclusive_bytes()` and charge each distinct palette once.
    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }
    pub fn exclusive_bytes(&self) -> u64 {
        self.resident_bytes - self.palette.resident_bytes()
    }
    pub fn palette(&self) -> &Arc<GpuSogPalette> {
        &self.palette
    }
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    [width, height]: [u32; 2],
    format: wgpu::TextureFormat,
    bytes: &[u8],
    stride: u32,
) -> wgpu::TextureView {
    let texture = create_texture(device, [width, height], format);
    queue.write_texture(
        texture.as_image_copy(),
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * stride),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    texture.create_view(&Default::default())
}

fn create_texture(
    device: &wgpu::Device,
    [width, height]: [u32; 2],
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("SOG unfiltered data texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// A fully charged GPU allocation that cannot be published before all texture
/// rows are written. Dropping it cancels the upload without retaining CPU data.
pub struct GpuSogUpload {
    source: GpuSogSource,
    allocated_bytes: u64,
    textures: Vec<wgpu::Texture>,
    plane: usize,
    row: u32,
}

impl GpuSogUpload {
    /// Newly reserved payload. A supplied shared palette is charged separately
    /// by its cache owner; a palette created by this upload is included here.
    pub fn resident_bytes(&self) -> u64 {
        self.allocated_bytes
    }
    pub fn count(&self) -> u32 {
        self.source.count()
    }
    pub fn is_complete(&self) -> bool {
        self.plane == self.textures.len()
    }

    /// Writes whole rows up to `allowance` bytes. Callers must provide the same
    /// validated source and enough allowance for at least one row. A smaller
    /// allowance makes no progress and never exceeds the requested byte bound.
    pub fn write(&mut self, queue: &wgpu::Queue, source: &PackedSog, allowance: usize) -> usize {
        let higher = source.higher_order();
        let data = [
            Some(source.means_lo()),
            Some(source.means_hi()),
            Some(source.quats()),
            Some(source.scales()),
            Some(source.sh0()),
            higher.map(|h| h.labels()),
            higher.map(|h| h.centroids()),
        ];
        let mut written = 0;
        while let Some(texture) = self.textures.get(self.plane) {
            let row_bytes = texture.width() as usize * 4;
            let rows = ((allowance - written) / row_bytes)
                .min((texture.height() - self.row) as usize) as u32;
            if rows == 0 {
                break;
            }
            let bytes = data[self.plane].map_or(&[0u8; 4][..], |t| t.texels());
            let start = self.row as usize * row_bytes;
            let size = rows as usize * row_bytes;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: self.row,
                        z: 0,
                    },
                    ..texture.as_image_copy()
                },
                &bytes[start..start + size],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes as u32),
                    rows_per_image: Some(rows),
                },
                wgpu::Extent3d {
                    width: texture.width(),
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
            written += size;
            self.row += rows;
            if self.row == texture.height() {
                self.plane += 1;
                self.row = 0;
            }
        }
        written
    }

    pub fn finish(self) -> Result<GpuSogSource, Self> {
        if self.is_complete() {
            Ok(self.source)
        } else {
            Err(self)
        }
    }
}
