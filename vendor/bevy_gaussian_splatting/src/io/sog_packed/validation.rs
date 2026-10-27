//! Resumable validation; incomplete/untrusted texels never become GPU sources.
use super::*;

/// Owns shared input texels until every logical source/palette row is checked.
/// Dropping this job cancels admission. Failed/completed jobs publish no further source.
pub struct PackedValidation {
    source: Option<PackedSog>,
    row: usize,
    palette_row: usize,
    palette_rows: usize,
    scale_len: usize,
    sh0_len: usize,
    shn_len: usize,
    valid_scale: [bool; 256],
}

/// Check metadata and texture envelopes without scanning whole textures.
/// Call `advance` with a bounded row allowance, yielding between calls.
pub fn begin_decode_rgba(
    meta: &SogMeta,
    textures: &HashMap<String, DataTexture>,
    max_sh_degree: u32,
    limits: DecodeLimits,
) -> Result<PackedValidation, SogError> {
    prepare_rgba(meta, textures, max_sh_degree, limits)
}

impl PackedValidation {
    pub(super) fn new(source: PackedSog, meta: &SogMeta) -> Self {
        let mut valid_scale = [false; 256];
        // At most 256 exponentials, not three for every splat. Invalid unused
        // finite entries remain legal, preserving the synchronous decoder contract.
        for (valid, value) in valid_scale.iter_mut().zip(&meta.scales.codebook) {
            let scale = value.exp();
            *valid = scale.is_finite() && scale > 0.0;
        }
        let palette_rows = source.higher_order.as_ref().map_or(0, |h| {
            h.palette_count as usize
                * match h.encoded_bands {
                    1 => 3,
                    2 => 8,
                    _ => 15,
                }
        });
        Self {
            source: Some(source),
            row: 0,
            palette_row: 0,
            palette_rows,
            scale_len: meta.scales.codebook.len(),
            sh0_len: meta.sh0.codebook.len(),
            shn_len: meta.sh_n.as_ref().map_or(0, |sh| sh.codebook.len()),
            valid_scale,
        }
    }

    /// Inspect at most `max_rows` logical splat/palette rows in total.
    /// Returns the source exactly once, after all checks; errors retire the job.
    pub fn advance(&mut self, max_rows: usize) -> Result<Option<PackedSog>, SogError> {
        match self.check_rows(max_rows) {
            Ok(true) => Ok(self.source.take()),
            Ok(false) => Ok(None),
            Err(error) => {
                self.source = None;
                Err(error)
            }
        }
    }

    pub(super) fn finish(mut self) -> Result<PackedSog, SogError> {
        self.advance(usize::MAX)?
            .ok_or_else(|| invalid("source", "incomplete validation"))
    }

    fn check_rows(&mut self, mut remaining: usize) -> Result<bool, SogError> {
        let Some(source) = &self.source else {
            return Ok(false);
        };
        let end = self
            .row
            .saturating_add(remaining)
            .min(source.count as usize);
        for row in self.row..end {
            rgb_indices(&source.scales, row, self.scale_len, "scales.codebook")?;
            rgb_indices(&source.sh0, row, self.sh0_len, "sh0.codebook")?;
            for &i in &source.scales.texels()[row * 4..row * 4 + 3] {
                if !self.valid_scale[i as usize] {
                    return Err(invalid(
                        "scales.codebook",
                        "scale must be finite and positive",
                    ));
                }
            }
            if !(252..=255).contains(&source.quats.texels()[row * 4 + 3]) {
                return Err(invalid("quats", "invalid largest-component tag"));
            }
            if let Some(h) = &source.higher_order {
                let label = &h.labels.texels()[row * 4..row * 4 + 2];
                if u32::from(u16::from_le_bytes([label[0], label[1]])) >= h.palette_count {
                    return Err(invalid("shN", "label references missing palette entry"));
                }
            }
        }
        remaining -= end - self.row;
        self.row = end;
        let end = self
            .palette_row
            .saturating_add(remaining)
            .min(self.palette_rows);
        if let Some(h) = &source.higher_order {
            for row in self.palette_row..end {
                rgb_indices(&h.centroids, row, self.shn_len, "shN.codebook")?;
            }
        }
        self.palette_row = end;
        Ok(self.row == source.count as usize && self.palette_row == self.palette_rows)
    }
}

fn rgb_indices(
    texture: &DataTexture,
    row: usize,
    length: usize,
    field: &'static str,
) -> Result<(), SogError> {
    if texture.texels()[row * 4..row * 4 + 3]
        .iter()
        .any(|&i| i as usize >= length)
    {
        return Err(invalid(field, "texture references missing codebook entry"));
    }
    Ok(())
}
