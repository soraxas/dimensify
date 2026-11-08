//! Bounded binary little-endian Gaussian PLY decoding across network chunks.
use bevy::prelude::Vec3;
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d, SphericalHarmonicCoefficients};
use ply_rs::{
    parser::Parser,
    ply::{ElementDef, Encoding, Property, PropertyAccess, PropertyType, ScalarType},
};
use std::io::Cursor;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_BATCH_ROWS: usize = 4096;

pub(crate) struct Batch {
    pub cloud: PlanarGaussian3d,
    pub count: usize,
    pub total: usize,
    /// Center bounds for this batch; combine batches for the complete bounds.
    pub bounds: (Vec3, Vec3),
    /// Three times the largest scale, for conservative renderer bounds.
    pub support: f32,
}

struct Schema {
    vertex: ElementDef,
    stride: usize,
    rest_count: usize,
}

pub(crate) struct Decoder {
    storage_limit: Option<u64>,
    header: Vec<u8>,
    line_start: usize,
    schema: Option<Schema>,
    partial: Vec<u8>,
    received: usize,
    decoded: usize,
    fallback: bool,
    finished: bool,
    failed: bool,
}

impl Decoder {
    pub(crate) fn new(storage_limit: Option<u64>) -> Self {
        Self {
            storage_limit,
            header: Vec::new(),
            line_start: 0,
            schema: None,
            partial: Vec::new(),
            received: 0,
            decoded: 0,
            fallback: false,
            finished: false,
            failed: false,
        }
    }

    pub(crate) fn is_decided(&self) -> bool {
        self.schema.is_some() || self.fallback
    }

    pub(crate) fn is_fallback(&self) -> bool {
        self.fallback
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<Vec<Batch>, String> {
        if self.failed || self.finished {
            return Err("Gaussian PLY decoder is no longer accepting input".into());
        }
        let result = self.push_inner(bytes);
        if result.is_err() {
            self.failed = true;
            self.header = Vec::new();
            self.partial = Vec::new();
        }
        result
    }

    fn push_inner(&mut self, mut bytes: &[u8]) -> Result<Vec<Batch>, String> {
        self.received = self
            .received
            .checked_add(bytes.len())
            .ok_or("PLY input size overflow")?;
        if self.fallback {
            return Ok(Vec::new());
        }
        while self.schema.is_none() {
            let Some((&byte, remaining)) = bytes.split_first() else {
                return Ok(Vec::new());
            };
            bytes = remaining;
            if self.header.len() == MAX_HEADER_BYTES {
                return Err("Gaussian PLY header exceeds the 64 KiB limit".into());
            }
            self.header.push(byte);
            if byte != b'\n' {
                continue;
            }
            let complete = self.header[self.line_start..].trim_ascii() == b"end_header";
            self.line_start = self.header.len();
            if complete {
                self.read_header()?;
                if self.fallback {
                    return Ok(Vec::new());
                }
            }
        }
        let schema = self.schema.as_ref().unwrap();
        let remaining_bytes = schema
            .vertex
            .count
            .saturating_sub(self.decoded)
            .checked_mul(schema.stride)
            .and_then(|size| size.checked_sub(self.partial.len()))
            .ok_or("Gaussian PLY record size overflow")?;
        if bytes.len() > remaining_bytes {
            return Err("Gaussian PLY has payload after its declared vertex count".into());
        }
        let mut batches = Vec::new();
        let mut rows = Vec::with_capacity(
            ((self.partial.len() + bytes.len()) / schema.stride).min(MAX_BATCH_ROWS),
        );
        while !bytes.is_empty() {
            let record = if self.partial.is_empty() && bytes.len() >= schema.stride {
                let (record, remaining) = bytes.split_at(schema.stride);
                bytes = remaining;
                record
            } else {
                let copied = (schema.stride - self.partial.len()).min(bytes.len());
                self.partial.extend_from_slice(&bytes[..copied]);
                bytes = &bytes[copied..];
                if self.partial.len() != schema.stride {
                    break;
                }
                &self.partial
            };
            let row = Parser::<PlyGaussian>::new()
                .read_little_endian_element(&mut Cursor::new(record), &schema.vertex)
                .map_err(|error| format!("Invalid Gaussian PLY payload: {error}"))?
                .convert(schema.rest_count, self.decoded)?;
            rows.push(row);
            self.decoded += 1;
            self.partial.clear();
            if rows.len() == MAX_BATCH_ROWS {
                batches.push(make_batch(std::mem::take(&mut rows), schema.vertex.count)?);
            }
        }
        if !rows.is_empty() {
            batches.push(make_batch(rows, schema.vertex.count)?);
        }
        Ok(batches)
    }

    fn read_header(&mut self) -> Result<(), String> {
        let header_bytes = std::mem::take(&mut self.header);
        validate_header_numbers(&header_bytes)?;
        let header = Parser::<PlyGaussian>::new()
            .read_header(&mut Cursor::new(&header_bytes))
            .map_err(|error| format!("Invalid Gaussian PLY header: {error}"))?;
        if header.elements.len() != 1 {
            return Err("Gaussian PLY must contain one vertex element".into());
        }
        let vertex = header
            .elements
            .get("vertex")
            .ok_or("Gaussian PLY is missing its vertex element")?;
        let (stride, rest_count) = validate_schema(vertex)?;
        let minimum_stride = if header.encoding == Encoding::Ascii {
            vertex.properties.len()
        } else {
            stride
        };
        if vertex
            .count
            .checked_mul(minimum_stride)
            .and_then(|body| body.checked_add(header_bytes.len()))
            .is_none_or(|size| size > isize::MAX as usize)
        {
            return Err("Gaussian PLY declared payload exceeds platform addressable size".into());
        }
        check_storage(vertex.count, self.storage_limit)?;
        if header.encoding != Encoding::BinaryLittleEndian {
            self.fallback = true;
        } else {
            self.schema = Some(Schema {
                vertex: vertex.clone(),
                stride,
                rest_count,
            });
        }
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> Result<(), String> {
        if self.failed {
            return Err("Gaussian PLY decoding previously failed".into());
        }
        if !self.fallback {
            let schema = self
                .schema
                .as_ref()
                .ok_or("Truncated Gaussian PLY header")?;
            if !self.partial.is_empty() || self.decoded != schema.vertex.count {
                return Err("Truncated Gaussian PLY payload".into());
            }
        }
        self.finished = true;
        Ok(())
    }
}

// ply-rs uses unchecked integer parsing and a usize cast in its header grammar.
// Guard only those numeric fields before handing the complete header to it.
fn validate_header_numbers(bytes: &[u8]) -> Result<(), String> {
    let header = std::str::from_utf8(bytes)
        .map_err(|error| format!("Invalid Gaussian PLY header: {error}"))?;
    for line in header.lines() {
        let mut fields = line.split_ascii_whitespace();
        match fields.next() {
            Some("element") => {
                if let (Some(_), Some(count)) = (fields.next(), fields.next()) {
                    count
                        .parse::<usize>()
                        .map_err(|_| "Invalid Gaussian PLY vertex count")?;
                }
            }
            Some("format") => {
                if let (Some(_), Some(version)) = (fields.next(), fields.next()) {
                    let (major, minor) = version
                        .split_once('.')
                        .ok_or("Invalid Gaussian PLY version")?;
                    major
                        .parse::<u16>()
                        .map_err(|_| "Invalid Gaussian PLY version")?;
                    minor
                        .parse::<u8>()
                        .map_err(|_| "Invalid Gaussian PLY version")?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_schema(vertex: &ElementDef) -> Result<(usize, usize), String> {
    for name in [
        "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "scale_0", "scale_1", "scale_2", "opacity",
        "rot_0", "rot_1", "rot_2", "rot_3",
    ] {
        let property = vertex.properties.get(name).ok_or_else(|| {
            format!("Gaussian PLY is missing {name}; ordinary point-cloud PLY is not supported")
        })?;
        if property.data_type != PropertyType::Scalar(ScalarType::Float) {
            return Err(format!("Gaussian PLY property {name} must be float32"));
        }
    }
    let mut stride = 0_usize;
    let mut rest_count = 0;
    for (name, property) in &vertex.properties {
        if let Some(suffix) = name.strip_prefix("f_rest_") {
            if suffix.parse::<usize>().map_or(true, |i| i >= 45)
                || property.data_type != PropertyType::Scalar(ScalarType::Float)
            {
                return Err(format!(
                    "Invalid Gaussian spherical harmonic property {name}"
                ));
            }
            rest_count += 1;
        }
        let size = match property.data_type {
            PropertyType::Scalar(ScalarType::Char | ScalarType::UChar) => 1,
            PropertyType::Scalar(ScalarType::Short | ScalarType::UShort) => 2,
            PropertyType::Scalar(ScalarType::Int | ScalarType::UInt | ScalarType::Float) => 4,
            PropertyType::Scalar(ScalarType::Double) => 8,
            PropertyType::List(_, _) => {
                return Err("Gaussian PLY list properties are not supported".into());
            }
        };
        stride = stride
            .checked_add(size)
            .ok_or("Gaussian PLY record size overflow")?;
    }
    if !matches!(rest_count, 0 | 9 | 24 | 45)
        || (0..rest_count).any(|i| !vertex.properties.contains_key(&format!("f_rest_{i}")))
    {
        return Err(
            "Gaussian PLY must provide contiguous SH0, SH1, SH2 or SH3 coefficients".into(),
        );
    }
    Ok((stride, rest_count))
}

fn check_storage(count: usize, storage_limit: Option<u64>) -> Result<(), String> {
    if count == 0 {
        return Err("Gaussian count must be nonzero".into());
    }
    // Preserve whole-file admission, including its alignment allowance.
    let size = count
        .checked_add(32 - count % 32)
        .and_then(|count| count.checked_mul(std::mem::size_of::<SphericalHarmonicCoefficients>()))
        .filter(|size| *size <= isize::MAX as usize)
        .ok_or("Gaussian buffers exceed this platform's addressable allocation size")?;
    if let Some(limit) = storage_limit
        && size as u64 > limit
    {
        return Err(format!(
            "{count} Gaussians require a {} MiB SH buffer; this GPU supports {} MiB per storage buffer",
            (size as u64).div_ceil(1024 * 1024),
            limit / (1024 * 1024)
        ));
    }
    Ok(())
}

// Keep authored channel-major SH separate until the degree is known. This also
// avoids indexing outside the compiled SH plane when a lower degree is enabled.
struct PlyGaussian {
    gaussian: Gaussian3d,
    rest: [f32; 45],
}

impl PropertyAccess for PlyGaussian {
    fn new() -> Self {
        Self {
            gaussian: Gaussian3d::default(),
            rest: [0.0; 45],
        }
    }

    fn set_property(&mut self, key: String, value: Property) {
        if let Some(index) = key
            .strip_prefix("f_rest_")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Property::Float(value) = value {
                self.rest[index] = value;
            }
        } else {
            self.gaussian.set_property(key, value);
        }
    }
}

impl PlyGaussian {
    fn convert(mut self, rest_count: usize, row: usize) -> Result<Gaussian3d, String> {
        let gaussian = &mut self.gaussian;
        let per_channel = rest_count / 3;
        for i in 0..rest_count {
            let channel = i / per_channel;
            let coefficient = i % per_channel + 1;
            if let Some(target) = gaussian
                .spherical_harmonic
                .coefficients
                .get_mut(coefficient * 3 + channel)
            {
                *target = self.rest[i];
            }
        }
        for scale in &mut gaussian.scale_opacity.scale {
            *scale = scale.exp();
        }
        let length = gaussian
            .rotation
            .rotation
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt();
        if !length.is_finite() || length <= f32::EPSILON {
            return Err("Gaussian PLY quaternion must be finite and nonzero".into());
        }
        for component in &mut gaussian.rotation.rotation {
            *component /= length;
        }
        if !Vec3::from_array(gaussian.position_visibility.position).is_finite()
            || !gaussian.rotation.rotation.iter().all(|x| x.is_finite())
            || !gaussian
                .scale_opacity
                .scale
                .iter()
                .all(|x| x.is_finite() && *x > 0.0)
            || !gaussian.scale_opacity.opacity.is_finite()
            || !gaussian
                .spherical_harmonic
                .coefficients
                .iter()
                .all(|x| x.is_finite())
            || !self.rest[..rest_count].iter().all(|x| x.is_finite())
        {
            return Err(format!(
                "Gaussian {row} contains non-finite or invalid attributes"
            ));
        }
        Ok(self.gaussian)
    }
}

fn make_batch(rows: Vec<Gaussian3d>, total: usize) -> Result<Batch, String> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut support = 0.0_f32;
    for row in &rows {
        let center = Vec3::from_array(row.position_visibility.position);
        min = min.min(center);
        max = max.max(center);
        support = support.max(Vec3::from_array(row.scale_opacity.scale).max_element() * 3.0);
    }
    if !support.is_finite() {
        return Err("Gaussian source bounds overflow".into());
    }
    Ok(Batch {
        count: rows.len(),
        cloud: rows.into(),
        total,
        bounds: (min, max),
        support,
    })
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;
