//! Explicit decoded ownership; packed sources never carry expanded float planes.
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d, io::sog_packed::PackedSog};
use std::sync::{Arc, Weak};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum SourceStorage {
    #[default]
    Expanded,
    Packed,
}

#[allow(
    clippy::large_enum_variant,
    reason = "Variants expose existing decoded ownership without another allocation."
)]
pub enum SourceData {
    Expanded(PlanarGaussian3d),
    Packed(Arc<PackedSog>),
    /// Stable original-file identity scopes shared palettes within one scene.
    PackedPage {
        original_file: usize,
        source: PackedSog,
    },
    /// Non-owning readiness mirror. The packed adapter owns the GPU allocation;
    /// streaming refreshes this descriptor before selection, not CPU texels.
    ResidentPacked(PackedSourceDescriptor),
}

#[derive(Clone, Copy)]
pub struct PackedSourceDescriptor {
    pub count: usize,
    pub group: (bool, usize),
    pub gpu_bytes: u64,
}

/// Admission envelope before WebP headers arrive: six RGBA attribute/label
/// planes with power-of-two padding, the full SH3 palette, and fixed codebooks.
/// Decode enforces this envelope; unusual over-padded files fail explicitly
/// rather than allocating beyond their reservation. After decode use exact bytes.
pub fn packed_reservation(count: usize) -> usize {
    count
        .checked_next_power_of_two()
        .and_then(|pixels| pixels.checked_mul(24))
        .and_then(|bytes| bytes.checked_add(4 * 1024 * 1024 + 8192))
        .unwrap_or(usize::MAX)
}

impl Default for SourceData {
    fn default() -> Self {
        Self::Expanded(PlanarGaussian3d::default())
    }
}
impl From<PlanarGaussian3d> for SourceData {
    fn from(value: PlanarGaussian3d) -> Self {
        Self::Expanded(value)
    }
}
impl From<Vec<Gaussian3d>> for SourceData {
    fn from(value: Vec<Gaussian3d>) -> Self {
        Self::Expanded(value.into())
    }
}
impl SourceData {
    /// Identify the shared palette used by a source.
    pub fn palette_group(&self, source_id: usize) -> (bool, usize) {
        match self {
            Self::PackedPage { original_file, .. } => (true, *original_file),
            Self::ResidentPacked(info) => info.group,
            _ => (false, source_id),
        }
    }
    /// Borrow packed texels, rejecting an expanded source.
    pub fn packed(&self) -> Result<&PackedSog, String> {
        match self {
            Self::Packed(source) => Ok(source),
            Self::PackedPage { source, .. } => Ok(source),
            Self::Expanded(_) => {
                Err("expanded source cannot be consumed by a packed adapter".into())
            }
            Self::ResidentPacked(_) => Err("packed CPU staging has been released".into()),
        }
    }
    /// Observe whole-file CPU recovery without retaining texels or a cache entry.
    /// Expiry follows the last staging/warm owner, including trims after upload.
    pub(super) fn packed_recovery(&self) -> Weak<PackedSog> {
        match self {
            Self::Packed(source) => Arc::downgrade(source),
            // The experimental page cache has separate ownership; unknown is
            // conservatively treated like no recoverable whole-file copy.
            _ => Weak::new(),
        }
    }
    /// Return the CPU bytes retained by this source.
    pub fn retained_bytes(&self) -> u64 {
        match self {
            Self::Packed(source) => {
                source.texture_bytes()
                    + std::mem::size_of::<PackedSog>() as u64
                    + 2 * std::mem::size_of::<usize>() as u64
            }
            Self::PackedPage { source, .. } => {
                source.texture_bytes() + std::mem::size_of::<PackedSog>() as u64
            }
            Self::Expanded(cloud) => {
                super::cloud_rows(cloud) as u64 * super::source_bytes_per_splat()
            }
            Self::ResidentPacked(_) => std::mem::size_of::<PackedSourceDescriptor>() as u64,
        }
    }
    /// Return the source bytes charged to cache admission.
    pub fn admission_bytes(&self) -> u64 {
        match self {
            Self::Packed(_) | Self::PackedPage { .. } => self.retained_bytes().max(
                bevy_gaussian_splatting::render::sog::GpuSogSource::required_bytes(
                    self.packed().expect("packed source"),
                ),
            ),
            Self::Expanded(_) => self.retained_bytes(),
            Self::ResidentPacked(info) => info.gpu_bytes,
        }
    }
    /// Borrow expanded planes, rejecting a packed source.
    pub fn expanded(&self) -> Result<&PlanarGaussian3d, String> {
        match self {
            Self::Expanded(cloud) => Ok(cloud),
            Self::Packed(_) | Self::PackedPage { .. } | Self::ResidentPacked(_) => {
                Err("packed source cannot be consumed by an expanded adapter".into())
            }
        }
    }
    /// Mutably borrow the expanded source planes.
    #[cfg(test)]
    pub fn expanded_mut(&mut self) -> &mut PlanarGaussian3d {
        match self {
            Self::Expanded(cloud) => cloud,
            Self::Packed(_) | Self::PackedPage { .. } | Self::ResidentPacked(_) => {
                panic!("expanded fixture")
            }
        }
    }
}
