//! Engine-independent SOG expansion shared by serial and worker decoding.
use serde::Deserialize;
use std::collections::HashMap;
#[derive(Debug)]
pub struct Expanded {
    pub positions: Vec<f32>,
    pub rotations: Vec<f32>,
    pub scales: Vec<f32>,
    pub harmonics: Vec<f32>,
}
#[derive(Debug, thiserror::Error)]
pub enum SogError {
    #[error("invalid SOG meta.json: {0}")]
    Meta(#[from] serde_json::Error),
    #[error("unsupported SOG version {0}, only version 2 is supported")]
    UnsupportedVersion(u64),
    #[error("SOG file '{0}' is missing")]
    MissingFile(String),
    #[error("SOG file '{file}' is not decodable WebP: {source}")]
    Image {
        file: String,
        source: image::ImageError,
    },
    #[error("SOG texture '{file}' has {got} pixels, count requires {needed}")]
    TextureTooSmall {
        file: String,
        needed: usize,
        got: usize,
    },
    #[error("SOG meta field '{field}' is invalid: {reason}")]
    Field { field: &'static str, reason: String },
    #[error("invalid .sog archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("reading .sog archive entry '{file}': {source}")]
    ZipEntry {
        file: String,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SogMeans {
    pub mins: [f64; 3],
    pub maxs: [f64; 3],
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SogCodebook {
    pub codebook: Vec<f32>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SogFiles {
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SogShN {
    pub count: usize,
    pub bands: usize,
    pub codebook: Vec<f32>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SogMeta {
    pub version: u64,
    pub count: usize,
    pub means: SogMeans,
    pub scales: SogCodebook,
    pub quats: SogFiles,
    pub sh0: SogCodebook,
    #[serde(rename = "shN", default)]
    pub sh_n: Option<SogShN>,
}

impl SogMeta {
    /// Files consumed by [`decode_expanded`] with the caller's SH capability.
    /// Use [`Self::referenced_files`] instead for complete dataset inventories.
    pub fn required_files_for_decode(&self, coefficients_per_channel: usize) -> Vec<&str> {
        self.files_with_higher_bands(self.decoded_sh_n(coefficients_per_channel))
    }

    /// Every WebP file referenced by this meta, in declaration order.
    pub fn referenced_files(&self) -> Vec<&str> {
        self.files_with_higher_bands(self.sh_n.as_ref())
    }

    fn decoded_sh_n(&self, coefficients_per_channel: usize) -> Option<&SogShN> {
        self.sh_n
            .as_ref()
            .filter(|sh_n| coefficients_per_channel > 1 && sh_n.bands > 0)
    }

    fn files_with_higher_bands<'a>(&'a self, sh_n: Option<&'a SogShN>) -> Vec<&'a str> {
        self.means
            .files
            .iter()
            .chain(&self.quats.files)
            .chain(&self.scales.files)
            .chain(&self.sh0.files)
            .chain(sh_n.into_iter().flat_map(|sh_n| &sh_n.files))
            .map(String::as_str)
            .collect()
    }
}

/// Parse and validate a SOG v2 `meta.json`.
///
/// Legacy v1 metas (no `version` field) and unknown future versions are
/// rejected rather than mis-decoded.
pub fn parse_meta(bytes: &[u8]) -> Result<SogMeta, SogError> {
    let version = serde_json::from_slice::<serde_json::Value>(bytes)?
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    if version != 2 {
        return Err(SogError::UnsupportedVersion(version));
    }

    let meta: SogMeta = serde_json::from_slice(bytes)?;

    let field_err = |field, reason: &str| SogError::Field {
        field,
        reason: reason.to_string(),
    };
    if meta.means.files.len() != 2 {
        return Err(field_err("means.files", "expected exactly 2 files"));
    }
    if meta.quats.files.is_empty() {
        return Err(field_err("quats.files", "expected at least 1 file"));
    }
    if meta.scales.files.is_empty() || meta.scales.codebook.is_empty() {
        return Err(field_err("scales", "missing files or codebook"));
    }
    if meta.sh0.files.is_empty() || meta.sh0.codebook.is_empty() {
        return Err(field_err("sh0", "missing files or codebook"));
    }
    if let Some(sh_n) = &meta.sh_n {
        if sh_n.files.len() != 2 {
            return Err(field_err("shN.files", "expected exactly 2 files"));
        }
        if sh_n.bands > 3 {
            return Err(field_err("shN.bands", "expected 0..=3 bands"));
        }
    }

    Ok(meta)
}

struct Rgba {
    data: Vec<u8>,
    width: usize,
    height: usize,
}

fn decode_webp(name: &str, files: &HashMap<String, Vec<u8>>) -> Result<Rgba, SogError> {
    let bytes = files
        .get(name)
        .ok_or_else(|| SogError::MissingFile(name.to_string()))?;
    let img = image::load_from_memory(bytes).map_err(|source| SogError::Image {
        file: name.to_string(),
        source,
    })?;
    let rgba = img.to_rgba8();
    let (width, height) = (rgba.width() as usize, rgba.height() as usize);
    Ok(Rgba {
        data: rgba.into_raw(),
        width,
        height,
    })
}

fn decode_webp_with_min_pixels(
    name: &str,
    files: &HashMap<String, Vec<u8>>,
    needed: usize,
) -> Result<Rgba, SogError> {
    let rgba = decode_webp(name, files)?;
    let got = rgba.width * rgba.height;
    if got < needed {
        return Err(SogError::TextureTooSmall {
            file: name.to_string(),
            needed,
            got,
        });
    }
    Ok(rgba)
}

/// Inverse of `logTransform(x) = sign(x) * ln(|x| + 1)`.
fn inv_log_transform(v: f64) -> f32 {
    let e = v.abs().exp() - 1.0;
    (if v < 0.0 { -e } else { e }) as f32
}

/// Unpack a smallest-three quaternion into `(w, x, y, z)`.
///
/// `tag` is the alpha byte; `tag - 252` selects which component was largest
/// (0..=3 -> w/x/y/z). Out-of-range tags decode to identity.
fn unpack_quat(px: u8, py: u8, pz: u8, tag: u8) -> [f32; 4] {
    if !(252..=255).contains(&tag) {
        return [1.0, 0.0, 0.0, 0.0];
    }
    let max_comp = (tag - 252) as usize;

    let unpack = |b: u8| (b as f32 / 255.0 * 2.0 - 1.0) / std::f32::consts::SQRT_2;

    const IDX: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];
    let mut comps = [0.0f32; 4];
    comps[IDX[max_comp][0]] = unpack(px);
    comps[IDX[max_comp][1]] = unpack(py);
    comps[IDX[max_comp][2]] = unpack(pz);

    let t = 1.0 - comps.iter().map(|c| c * c).sum::<f32>();
    comps[max_comp] = t.max(0.0).sqrt();
    comps
}

/// Decode SOG v2 content into a engine-independent planar cloud.
///
/// `files` maps the file names referenced by `meta` (e.g. `means_l.webp`) to
/// their raw bytes. Exactly `meta.count` splats are decoded; trailing texture
/// pixels are ignored. `shN` data decodes into the higher-band SH
/// coefficients supported by the build's SH degree. Unused coefficients and
/// storage padding stay zero. Unused higher-band files need not be supplied;
/// missing or malformed files that the build consumes remain load errors.
pub fn decode_expanded(
    meta: &SogMeta,
    files: &HashMap<String, Vec<u8>>,
    coefficients_per_channel: usize,
) -> Result<Expanded, SogError> {
    let count = meta.count;
    let codebook = |book: &[f32], index: u8| book.get(index as usize).copied().unwrap_or(0.0);

    if ![1, 4, 9, 16, 25].contains(&coefficients_per_channel) {
        return Err(SogError::Field {
            field: "SH",
            reason: "invalid coefficient count".into(),
        });
    }
    let coefficients = (coefficients_per_channel * 3).div_ceil(4) * 4;
    let padded = count
        .checked_add(32 - count % 32)
        .ok_or_else(|| SogError::Field {
            field: "count",
            reason: "padding overflow".into(),
        })?;
    let mut cloud = Expanded {
        positions: vec![0.; padded * 4],
        rotations: vec![0.; padded * 4],
        scales: vec![0.; padded * 4],
        harmonics: vec![0.; padded * coefficients],
    };
    for position in cloud.positions.as_chunks_mut::<4>().0 {
        position[3] = 1.;
    }

    // means: 16-bit lo|hi per axis, lerped between mins/maxs, then inverse
    // log transform. Buffers drop at the end of each block to bound peak
    // memory on large scenes.
    {
        let lo = decode_webp_with_min_pixels(&meta.means.files[0], files, count)?;
        let hi = decode_webp_with_min_pixels(&meta.means.files[1], files, count)?;
        let mins = meta.means.mins;
        let scale = [
            meta.means.maxs[0] - mins[0],
            meta.means.maxs[1] - mins[1],
            meta.means.maxs[2] - mins[2],
        ];

        for i in 0..count {
            let o = i * 4;
            for axis in 0..3 {
                let fixed = lo.data[o + axis] as u16 | ((hi.data[o + axis] as u16) << 8);
                let v = mins[axis] + scale[axis] * (fixed as f64 / 65535.0);
                cloud.positions[i * 4 + axis] = inv_log_transform(v);
            }
        }
    }

    // quats: smallest-three packing, decoded to (w, x, y, z) like PLY
    // rot_0..rot_3.
    {
        let quats = decode_webp_with_min_pixels(&meta.quats.files[0], files, count)?;
        for i in 0..count {
            let o = i * 4;
            cloud.rotations[i * 4..i * 4 + 4].copy_from_slice(&unpack_quat(
                quats.data[o],
                quats.data[o + 1],
                quats.data[o + 2],
                quats.data[o + 3],
            ));
        }
    }

    // scales: preserve authored anisotropy by exponentiating each log scale
    // directly, without the PLY path's mean-relative variance clamp.
    {
        let scales = decode_webp_with_min_pixels(&meta.scales.files[0], files, count)?;
        // Byte indices can reference at most 256 values. Resolve their scales
        // once, retaining errors until an entry is actually used so unused bad
        // values remain legal and diagnostics identify the first affected splat.
        let scale_codebook: [Result<f32, &str>; 256] = std::array::from_fn(|index| {
            let log = meta
                .scales
                .codebook
                .get(index)
                .copied()
                .ok_or("missing codebook entry")?;
            if !log.is_finite() {
                return Err("log scale must be finite");
            }
            let scale = log.exp();
            if !scale.is_finite() || scale <= 0.0 {
                return Err("exponentiated scale must be finite and positive");
            }
            Ok(scale)
        });
        for i in 0..count {
            let o = i * 4;
            for axis in 0..3 {
                let index = scales.data[o + axis] as usize;
                let scale_err = |reason: &str| SogError::Field {
                    field: "scales.codebook",
                    reason: format!("index {index} (splat {i}, axis {axis}): {reason}"),
                };
                cloud.scales[i * 4 + axis] = scale_codebook[index].map_err(scale_err)?;
            }
        }
    }

    // sh0: RGB index the DC coefficient codebook; alpha is sigmoid-encoded
    // opacity (the PLY path stores sigmoid(logit), which round-trips to
    // alpha / 255).
    {
        let sh0 = decode_webp_with_min_pixels(&meta.sh0.files[0], files, count)?;
        for i in 0..count {
            let o = i * 4;
            for channel in 0..3 {
                cloud.harmonics[i * coefficients + channel] =
                    codebook(&meta.sh0.codebook, sh0.data[o + channel]);
            }
            cloud.scales[i * 4 + 3] = sh0.data[o + 3] as f32 / 255.0;
        }
    }

    // shN (optional): per-splat 16-bit label into a centroid palette laid out
    // 64 palette entries per row, `coeffs` pixels per entry. Coefficients
    // beyond the build's SH degree stay zero-filled.
    if let Some(sh_n) = &meta.sh_n {
        let coeffs = [0usize, 3, 8, 15][sh_n.bands.min(3)];
        let has_room = coefficients_per_channel > 1;
        if coeffs > 0 && has_room {
            let centroids = decode_webp(&sh_n.files[0], files)?;
            let labels = decode_webp_with_min_pixels(&sh_n.files[1], files, count)?;
            let expected_width = 64 * coeffs;
            if centroids.width != expected_width {
                return Err(SogError::Field {
                    field: "shN",
                    reason: format!(
                        "centroids texture width {} does not match expected {expected_width} for {}-band palette",
                        centroids.width, sh_n.bands
                    ),
                });
            }

            for i in 0..count {
                let o = i * 4;
                let label = labels.data[o] as usize | ((labels.data[o + 1] as usize) << 8);
                if label >= sh_n.count {
                    continue;
                }
                let (cx0, cy) = (label % 64 * coeffs, label / 64);
                if cy >= centroids.height {
                    continue;
                }
                for coeff in 0..coeffs {
                    let pixel = (cy * centroids.width + cx0 + coeff) * 4;
                    for channel in 0..3 {
                        // interleaved layout: coefficient-major, channel-minor
                        let idx = (coeff + 1) * 3 + channel;
                        if idx < coefficients_per_channel * 3 {
                            cloud.harmonics[i * coefficients + idx] =
                                codebook(&sh_n.codebook, centroids.data[pixel + channel]);
                        }
                    }
                }
            }
        }
    }

    Ok(cloud)
}

#[cfg(test)]
mod tests;
