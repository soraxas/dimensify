//! Quantized SOG v2 source ownership, independent of the compiled float-cloud SH layout.
//!
//! Only the WebP image codec runs here. Attribute dequantization and per-view SH
//! evaluation belong to GPU processing. These are data textures: no sRGB decode,
//! premultiplication, EXIF orientation, filtering or mipmaps are permitted.

use std::{collections::HashMap, sync::Arc};

use super::sog::{SogError, SogMeta};

mod validation;
pub use validation::{PackedValidation, begin_decode_rgba};

/// Caller-selected device and resident-texel bounds, not a total process-memory cap.
/// Compressed inputs, codec scratch and transient RGB-to-RGBA conversion are additional.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    pub max_texture_dimension: u32,
    pub max_texture_bytes: u64,
}

/// Immutable RGBA8 data. Clones share storage, including palettes shared by splats.
#[derive(Clone, Debug)]
pub struct DataTexture {
    dimensions: [u32; 2],
    // Keep the admitted Vec allocation: converting Vec into Arc<[u8]> copies
    // the whole texture again and defeats bounded browser admission.
    texels: Arc<Vec<u8>>,
}

impl DataTexture {
    /// Wrap externally decoded data texels, never image/display-converted pixels.
    /// The caller must reserve memory before decoding; source validation still
    /// enforces device dimensions and aggregate bytes in `decode_rgba`.
    pub fn from_rgba(width: u32, height: u32, texels: Vec<u8>) -> Result<Self, SogError> {
        let bytes = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|n| n.checked_mul(4));
        if width == 0 || height == 0 || bytes != Some(texels.len() as u64) {
            return Err(invalid(
                "texture",
                "RGBA byte count differs from dimensions",
            ));
        }
        Ok(Self {
            dimensions: [width, height],
            texels: texels.into(),
        })
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }
    pub fn texels(&self) -> &[u8] {
        &self.texels
    }
    pub fn byte_len(&self) -> u64 {
        self.texels.len() as u64
    }
}

/// Raw higher-band palette and 16-bit per-splat labels, never expanded coefficients.
#[derive(Clone, Debug)]
pub struct HigherOrder {
    bands: u32,
    encoded_bands: u32,
    palette_count: u32,
    labels: DataTexture,
    centroids: DataTexture,
    codebook: [f32; 256],
}

impl HigherOrder {
    pub fn bands(&self) -> u32 {
        self.bands
    }
    pub fn encoded_bands(&self) -> u32 {
        self.encoded_bands
    }
    pub fn palette_count(&self) -> u32 {
        self.palette_count
    }
    pub fn labels(&self) -> &DataTexture {
        &self.labels
    }
    pub fn centroids(&self) -> &DataTexture {
        &self.centroids
    }
    pub fn codebook(&self) -> &[f32; 256] {
        &self.codebook
    }
}

/// A validated immutable source. Logical count excludes texture padding.
#[derive(Clone, Debug)]
pub struct PackedSog {
    count: u32,
    means_min: [f32; 3],
    means_max: [f32; 3],
    means_lo: DataTexture,
    means_hi: DataTexture,
    quats: DataTexture,
    scales: DataTexture,
    sh0: DataTexture,
    scale_codebook: [f32; 256],
    sh0_codebook: [f32; 256],
    higher_order: Option<HigherOrder>,
    texture_bytes: u64,
}

impl PackedSog {
    pub fn count(&self) -> u32 {
        self.count
    }
    pub fn means_min(&self) -> [f32; 3] {
        self.means_min
    }
    pub fn means_max(&self) -> [f32; 3] {
        self.means_max
    }
    pub fn means_lo(&self) -> &DataTexture {
        &self.means_lo
    }
    pub fn means_hi(&self) -> &DataTexture {
        &self.means_hi
    }
    pub fn quats(&self) -> &DataTexture {
        &self.quats
    }
    pub fn scales(&self) -> &DataTexture {
        &self.scales
    }
    pub fn sh0(&self) -> &DataTexture {
        &self.sh0
    }
    pub fn scale_codebook(&self) -> &[f32; 256] {
        &self.scale_codebook
    }
    pub fn sh0_codebook(&self) -> &[f32; 256] {
        &self.sh0_codebook
    }
    pub fn higher_order(&self) -> Option<&HigherOrder> {
        self.higher_order.as_ref()
    }
    /// Referenced texture payload; excludes codebooks/metadata and codec scratch.
    /// A range copy shares its palette, so an owner retaining multiple ranges
    /// charges that palette once per original source, not once per range.
    pub fn texture_bytes(&self) -> u64 {
        self.texture_bytes
    }

    /// Per-splat planes, including row padding, without the shared SH palette.
    pub fn splat_texture_bytes(&self) -> u64 {
        self.texture_bytes - self.palette_texture_bytes()
    }

    /// Palette payload shared by clones and range copies of this source.
    pub fn palette_texture_bytes(&self) -> u64 {
        self.higher_order
            .as_ref()
            .map_or(0, |h| h.centroids.byte_len())
    }

    /// Reuse the palette retained by another range after refetching the same
    /// immutable source. Validate metadata and bytes before sharing; different
    /// palettes cannot silently change colours. No splat planes are shared.
    pub fn reuse_palette(&mut self, other: &Self) -> Result<(), SogError> {
        match (&mut self.higher_order, &other.higher_order) {
            (None, None) => Ok(()),
            (Some(own), Some(old))
                if own.encoded_bands == old.encoded_bands
                    && own.palette_count == old.palette_count
                    && own.codebook == old.codebook
                    && own.centroids.dimensions == old.centroids.dimensions
                    && (Arc::ptr_eq(&own.centroids.texels, &old.centroids.texels)
                        || own.centroids.texels == old.centroids.texels) =>
            {
                own.centroids = old.centroids.clone();
                Ok(())
            }
            _ => Err(invalid("palette", "refetched source palette differs")),
        }
    }

    /// Copy an exact logical range into independent per-splat planes. The SH
    /// palette remains shared; no original per-splat allocation is retained.
    /// Local index zero refers to `range.start` in the original source.
    ///
    /// Checks the complete referenced payload and device dimensions before
    /// allocation. The caller must also admit coexistence with the original
    /// decoded file and any other live ranges. This does not perform eviction.
    pub fn copy_range(
        &self,
        range: std::ops::Range<u32>,
        limits: DecodeLimits,
    ) -> Result<Self, SogError> {
        if range.start >= range.end || range.end > self.count {
            return Err(invalid("range", "expected non-empty logical source range"));
        }
        let count = range.end - range.start;
        let width = count.min(256);
        let height = count.div_ceil(width);
        let dimensions = [width, height];
        if dimensions.iter().any(|&d| d > limits.max_texture_dimension)
            || self.higher_order.as_ref().is_some_and(|h| {
                h.centroids
                    .dimensions
                    .iter()
                    .any(|&d| d > limits.max_texture_dimension)
            })
        {
            return Err(invalid("range", "textures exceed device dimensions"));
        }
        let plane_bytes = u64::from(width) * u64::from(height) * 4;
        let texture_bytes = plane_bytes * if self.higher_order.is_some() { 6 } else { 5 }
            + self.palette_texture_bytes();
        if texture_bytes > limits.max_texture_bytes {
            return Err(invalid("range", "textures exceed admitted byte allowance"));
        }
        // These conversions also validate the allocation/slice lengths on
        // wasm32 before any page data is allocated.
        let plane_bytes = usize::try_from(plane_bytes)
            .map_err(|_| invalid("range", "plane exceeds address space"))?;
        let start = usize::try_from(u64::from(range.start) * 4)
            .map_err(|_| invalid("range", "offset exceeds address space"))?;
        let end = usize::try_from(u64::from(range.end) * 4)
            .map_err(|_| invalid("range", "end exceeds address space"))?;
        let copy = |texture: &DataTexture| {
            let mut texels = vec![0; plane_bytes];
            texels[..end - start].copy_from_slice(&texture.texels()[start..end]);
            DataTexture {
                dimensions,
                texels: texels.into(),
            }
        };
        Ok(Self {
            count,
            means_lo: copy(&self.means_lo),
            means_hi: copy(&self.means_hi),
            quats: copy(&self.quats),
            scales: copy(&self.scales),
            sh0: copy(&self.sh0),
            higher_order: self.higher_order.as_ref().map(|higher| HigherOrder {
                labels: copy(&higher.labels),
                ..higher.clone()
            }),
            texture_bytes,
            ..self.clone()
        })
    }
}

fn invalid(field: &'static str, reason: impl Into<String>) -> SogError {
    SogError::Field {
        field,
        reason: reason.into(),
    }
}

fn filename<'a>(
    files: &'a [String],
    index: usize,
    field: &'static str,
) -> Result<&'a str, SogError> {
    files
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| invalid(field, "missing texture name"))
}

fn codebook(values: &[f32], field: &'static str) -> Result<[f32; 256], SogError> {
    if values.is_empty() || values.len() > 256 || values.iter().any(|v| !v.is_finite()) {
        return Err(invalid(field, "expected 1..=256 finite entries"));
    }
    let mut book = [0.0; 256];
    book[..values.len()].copy_from_slice(values);
    Ok(book)
}

fn texture(
    name: &str,
    files: &HashMap<String, Vec<u8>>,
    limits: DecodeLimits,
    remaining: &mut u64,
) -> Result<DataTexture, SogError> {
    let bytes = files
        .get(name)
        .ok_or_else(|| SogError::MissingFile(name.into()))?;
    let (header, rgba) = sog_codec::decode(bytes, limits.max_texture_dimension, *remaining)
        .map_err(|e| match e {
            sog_codec::Error::Image(source) => SogError::Image {
                file: name.into(),
                source,
            },
            sog_codec::Error::Invalid(reason) => invalid("texture", format!("{name}: {reason}")),
        })?;
    *remaining -= header.rgba_bytes;
    DataTexture::from_rgba(header.width, header.height, rgba)
}

/// Decode only codec pixels, retaining authored SH up to an explicit capability.
/// This API does not inherit the legacy renderer's compile-time SH degree.
/// Invalid input returns an error before a source can be bound to GPU shaders.
pub fn decode(
    meta: &SogMeta,
    files: &HashMap<String, Vec<u8>>,
    max_sh_degree: u32,
    limits: DecodeLimits,
) -> Result<PackedSog, SogError> {
    build(meta, max_sh_degree, limits, |name, remaining| {
        texture(name, files, limits, remaining)
    })?
    .finish()
}

/// Validate immutable externally decoded texels against the identical codec
/// contract. Browser/native workers must preserve RGBA bytes exactly, including
/// RGB under transparent alpha. Clones share texels; no float expansion occurs.
pub fn decode_rgba(
    meta: &SogMeta,
    textures: &HashMap<String, DataTexture>,
    max_sh_degree: u32,
    limits: DecodeLimits,
) -> Result<PackedSog, SogError> {
    begin_decode_rgba(meta, textures, max_sh_degree, limits)?.finish()
}

fn prepare_rgba(
    meta: &SogMeta,
    textures: &HashMap<String, DataTexture>,
    max_sh_degree: u32,
    limits: DecodeLimits,
) -> Result<PackedValidation, SogError> {
    build(meta, max_sh_degree, limits, |name, remaining| {
        let data = textures
            .get(name)
            .ok_or_else(|| SogError::MissingFile(name.into()))?;
        if data
            .dimensions
            .iter()
            .any(|&d| d > limits.max_texture_dimension)
            || data.byte_len() > *remaining
        {
            return Err(invalid(
                "texture",
                "external texels exceed dimension/byte allowance",
            ));
        }
        *remaining -= data.byte_len();
        Ok(data.clone())
    })
}

fn build(
    meta: &SogMeta,
    max_sh_degree: u32,
    limits: DecodeLimits,
    mut texture: impl FnMut(&str, &mut u64) -> Result<DataTexture, SogError>,
) -> Result<PackedValidation, SogError> {
    if meta.version != 2 {
        return Err(SogError::UnsupportedVersion(meta.version));
    }
    if max_sh_degree > 3 {
        return Err(invalid("SH degree", "supported range is 0..=3"));
    }
    let count =
        u32::try_from(meta.count).map_err(|_| invalid("count", "exceeds GPU index range"))?;
    let means_min = meta.means.mins.map(|v| v as f32);
    let means_max = meta.means.maxs.map(|v| v as f32);
    if (0..3).any(|i| {
        !means_min[i].is_finite()
            || !means_max[i].is_finite()
            || means_min[i] > means_max[i]
            || !(means_min[i].abs().max(means_max[i].abs()).exp()).is_finite()
    }) {
        return Err(invalid(
            "means",
            "bounds must decode to finite ordered f32 positions",
        ));
    }
    let scale_codebook = codebook(&meta.scales.codebook, "scales.codebook")?;
    let sh0_codebook = codebook(&meta.sh0.codebook, "sh0.codebook")?;
    let mut remaining = limits.max_texture_bytes;
    let mut read = |name| texture(name, &mut remaining);
    let means_lo = read(filename(&meta.means.files, 0, "means.files")?)?;
    let means_hi = read(filename(&meta.means.files, 1, "means.files")?)?;
    let quats = read(filename(&meta.quats.files, 0, "quats.files")?)?;
    let scales = read(filename(&meta.scales.files, 0, "scales.files")?)?;
    let sh0 = read(filename(&meta.sh0.files, 0, "sh0.files")?)?;
    let dimensions = means_lo.dimensions;
    if u64::from(dimensions[0]) * u64::from(dimensions[1]) < u64::from(count) {
        return Err(invalid("count", "logical count exceeds texture capacity"));
    }
    for base in [&means_hi, &quats, &scales, &sh0] {
        if base.dimensions != dimensions {
            return Err(invalid("texture", "base texture dimensions differ"));
        }
    }
    let higher_order = if let Some(sh) = meta
        .sh_n
        .as_ref()
        .filter(|sh| max_sh_degree > 0 && sh.bands > 0)
    {
        let coeffs = match sh.bands {
            1 => 3,
            2 => 8,
            3 => 15,
            _ => return Err(invalid("shN.bands", "supported range is 0..=3")),
        };
        if sh.count == 0 || sh.count > 65536 {
            return Err(invalid("shN.count", "palette must have 1..=65536 entries"));
        }
        let book = codebook(&sh.codebook, "shN.codebook")?;
        let centroids = read(filename(&sh.files, 0, "shN.files")?)?;
        let labels = read(filename(&sh.files, 1, "shN.files")?)?;
        if centroids.dimensions[0] != 64 * coeffs
            || (u64::from(centroids.dimensions[1]) * 64) < sh.count as u64
        {
            return Err(invalid(
                "shN",
                "palette dimensions do not cover declared entries",
            ));
        }
        if labels.dimensions != dimensions {
            return Err(invalid("shN", "label dimensions differ from base textures"));
        }
        Some(HigherOrder {
            bands: max_sh_degree.min(sh.bands as u32),
            encoded_bands: sh.bands as u32,
            palette_count: sh.count as u32,
            labels,
            centroids,
            codebook: book,
        })
    } else {
        None
    };
    Ok(PackedValidation::new(
        PackedSog {
            count,
            means_min,
            means_max,
            means_lo,
            means_hi,
            quats,
            scales,
            sh0,
            scale_codebook,
            sh0_codebook,
            higher_order,
            texture_bytes: limits.max_texture_bytes - remaining,
        },
        meta,
    ))
}

#[cfg(test)]
mod range_ownership_tests {
    use super::*;

    #[test]
    fn ranges_do_not_pin_original_splat_planes_and_release_the_last_palette() {
        let plane = || DataTexture::from_rgba(2, 2, vec![0; 16]).unwrap();
        let source = PackedSog {
            count: 3,
            means_min: [-1.; 3],
            means_max: [1.; 3],
            means_lo: plane(),
            means_hi: plane(),
            quats: DataTexture::from_rgba(2, 2, [128, 128, 128, 255].repeat(4)).unwrap(),
            scales: plane(),
            sh0: plane(),
            scale_codebook: [0.; 256],
            sh0_codebook: [0.; 256],
            higher_order: Some(HigherOrder {
                bands: 1,
                encoded_bands: 1,
                palette_count: 1,
                labels: plane(),
                centroids: DataTexture::from_rgba(192, 1, vec![0; 768]).unwrap(),
                codebook: [0.; 256],
            }),
            texture_bytes: 864,
        };
        let limits = DecodeLimits {
            max_texture_dimension: 4096,
            max_texture_bytes: 1 << 20,
        };
        let planes = [
            &source.means_lo,
            &source.means_hi,
            &source.quats,
            &source.scales,
            &source.sh0,
            &source.higher_order.as_ref().unwrap().labels,
        ]
        .map(|texture| Arc::downgrade(&texture.texels));
        let palette = Arc::downgrade(&source.higher_order.as_ref().unwrap().centroids.texels);
        let a = source.copy_range(0..2, limits).unwrap();
        let b = source.copy_range(2..3, limits).unwrap();
        drop(source);
        assert!(planes.iter().all(|plane| plane.upgrade().is_none()));
        assert!(palette.upgrade().is_some());
        drop(a);
        assert!(palette.upgrade().is_some());
        drop(b);
        assert!(palette.upgrade().is_none());
    }
}
