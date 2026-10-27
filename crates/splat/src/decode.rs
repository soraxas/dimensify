use super::DecodedSplat;
use bevy::prelude::*;
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d, io::sog::decode_sog_bundle};
use ply_rs::{
    parser::Parser,
    ply::{Property, PropertyAccess, PropertyType, ScalarType},
};
use std::io::{Cursor, Read};

pub(super) const MAX_BYTES: usize = 256 * 1024 * 1024;

/// Decode standard Gaussian PLY or a bundled SOG v2 archive with bounded input.
/// Ordinary point-cloud PLY and non-finite Gaussian attributes are rejected.
pub fn decode_splat(name: &str, bytes: &[u8]) -> Result<DecodedSplat, String> {
    decode_for_device(name, bytes, None)
}

/// Validate storage requirements before decoding for an optional render device.
pub(super) fn decode_for_device(
    name: &str,
    bytes: &[u8],
    storage_limit: Option<u64>,
) -> Result<DecodedSplat, String> {
    if bytes.len() > MAX_BYTES {
        return Err("File exceeds the 256 MiB input limit".into());
    }
    let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let (cloud, count) = match extension.as_str() {
        "ply" => decode_ply(bytes, storage_limit)?,
        "sog" => {
            let count = sog_count(bytes, storage_limit)?;
            (
                decode_sog_bundle(bytes).map_err(|e| format!("Invalid SOG: {e}"))?,
                count,
            )
        }
        _ => return Err("Choose a Gaussian .ply or bundled .sog file".into()),
    };
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for i in 0..count {
        let position = Vec3::from_array(cloud.position_visibility[i].position);
        if !position.is_finite()
            || !cloud.rotation[i].rotation.iter().all(|x| x.is_finite())
            || !cloud.scale_opacity[i]
                .scale
                .iter()
                .all(|x| x.is_finite() && *x > 0.)
            || !cloud.scale_opacity[i].opacity.is_finite()
            || !cloud.spherical_harmonic[i]
                .coefficients
                .iter()
                .all(|x| x.is_finite())
        {
            return Err(format!(
                "Gaussian {i} contains non-finite or invalid attributes"
            ));
        }
        min = min.min(position);
        max = max.max(position);
    }
    Ok(DecodedSplat {
        cloud,
        count,
        bounds: (min, max),
    })
}

fn check_count(count: usize, storage_limit: Option<u64>) -> Result<usize, String> {
    if count == 0 {
        return Err("Gaussian count must be nonzero".into());
    }
    // Account for the codec's alignment padding and the largest renderer plane.
    let bytes = count
        .checked_add(32 - count % 32)
        .and_then(|n| {
            n.checked_mul(std::mem::size_of::<
                bevy_gaussian_splatting::SphericalHarmonicCoefficients,
            >())
        })
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or("Gaussian buffers exceed this platform's addressable allocation size")?;
    if let Some(limit) = storage_limit
        && bytes as u64 > limit
    {
        return Err(format!(
            "{count} Gaussians require a {} MiB SH buffer; this GPU supports {} MiB per storage buffer",
            (bytes as u64).div_ceil(1024 * 1024),
            limit / (1024 * 1024)
        ));
    }
    Ok(count)
}

// Match Graphdeco's channel-major higher SH layout, excluding each DC term.
struct PlyGaussian(Gaussian3d);
impl PropertyAccess for PlyGaussian {
    fn new() -> Self {
        Self(Gaussian3d::default())
    }
    fn set_property(&mut self, key: String, value: Property) {
        if let Some(index) = key
            .strip_prefix("f_rest_")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if let Property::Float(value) = value {
                self.0.spherical_harmonic.coefficients[3 + index] = value;
            }
        } else {
            self.0.set_property(key, value);
        }
    }
}

fn decode_ply(
    bytes: &[u8],
    storage_limit: Option<u64>,
) -> Result<(PlanarGaussian3d, usize), String> {
    let parser = Parser::<PlyGaussian>::new();
    let mut reader = Cursor::new(bytes);
    let header = parser
        .read_header(&mut reader)
        .map_err(|e| format!("Invalid Gaussian PLY header: {e}"))?;
    if header.elements.len() != 1 {
        return Err("Gaussian PLY must contain one vertex element".into());
    }
    let vertex = header
        .elements
        .get("vertex")
        .ok_or("Gaussian PLY is missing its vertex element")?;
    let count = check_count(vertex.count, storage_limit)?;
    // Every scalar consumes at least one byte in either supported PLY encoding.
    // Reject a fabricated count before ply-rs reserves its vertex vector.
    if count
        .checked_mul(vertex.properties.len())
        .is_none_or(|minimum| minimum > bytes.len().saturating_sub(reader.position() as usize))
    {
        return Err("Gaussian PLY payload is too small for its declared vertex count".into());
    }
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
    for (name, property) in &vertex.properties {
        if let Some(suffix) = name.strip_prefix("f_rest_")
            && (suffix.parse::<usize>().map_or(true, |i| i >= 45)
                || property.data_type != PropertyType::Scalar(ScalarType::Float))
        {
            return Err(format!(
                "Invalid Gaussian spherical harmonic property {name}"
            ));
        }
        if matches!(property.data_type, PropertyType::List(_, _)) {
            return Err("Gaussian PLY list properties are not supported".into());
        }
    }
    let rest_count = vertex
        .properties
        .keys()
        .filter(|name| name.starts_with("f_rest_"))
        .count();
    if !matches!(rest_count, 0 | 9 | 24 | 45)
        || (0..rest_count).any(|i| !vertex.properties.contains_key(&format!("f_rest_{i}")))
    {
        return Err(
            "Gaussian PLY must provide contiguous SH0, SH1, SH2 or SH3 coefficients".into(),
        );
    }
    let values = parser
        .read_payload_for_element(&mut reader, vertex, &header)
        .map_err(|e| format!("Invalid Gaussian PLY payload: {e}"))?;
    let mut gaussians: Vec<Gaussian3d> = values.into_iter().map(|g| g.0).collect();
    for g in &mut gaussians {
        let authored = g.spherical_harmonic.coefficients;
        let per_channel = rest_count / 3;
        for i in 0..rest_count {
            let channel = i / per_channel;
            let coefficient = i % per_channel + 1;
            g.spherical_harmonic
                .set(coefficient * 3 + channel, authored[3 + i]);
        }
        for scale in &mut g.scale_opacity.scale {
            *scale = scale.exp();
        }
        let length = g
            .rotation
            .rotation
            .iter()
            .map(|x| x * x)
            .sum::<f32>()
            .sqrt();
        if !length.is_finite() || length <= f32::EPSILON {
            return Err("Gaussian PLY quaternion must be finite and nonzero".into());
        }
        for x in &mut g.rotation.rotation {
            *x /= length;
        }
    }
    gaussians.resize(count.next_multiple_of(32), Gaussian3d::default());
    Ok((gaussians.into(), count))
}

fn sog_count(bytes: &[u8], storage_limit: Option<u64>) -> Result<usize, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Invalid SOG archive: {e}"))?;
    let mut names = std::collections::HashSet::new();
    let mut meta_index = None;
    let mut expanded_size = 0_u64;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        expanded_size = expanded_size.saturating_add(entry.size());
        if expanded_size > 1024 * 1024 * 1024 {
            return Err("SOG expanded archive exceeds the 1 GiB limit".into());
        }
        let name = entry
            .name()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        if !names.insert(name.clone()) {
            return Err(format!("Ambiguous SOG archive entry: {name}"));
        }
        if name == "meta.json" {
            if entry.size() > 2 * 1024 * 1024 {
                return Err("SOG metadata exceeds the 2 MiB limit".into());
            }
            meta_index = Some(i);
        }
    }
    let mut file = archive
        .by_index(meta_index.ok_or("SOG archive is missing meta.json")?)
        .map_err(|e| e.to_string())?;
    let mut metadata = Vec::new();
    file.read_to_end(&mut metadata).map_err(|e| e.to_string())?;
    drop(file);
    let meta = bevy_gaussian_splatting::io::sog::parse_meta(&metadata)
        .map_err(|e| format!("Invalid SOG metadata: {e}"))?;
    let count = check_count(meta.count, storage_limit)?;
    // Validate image headers before the codec allocates its expanded arrays.
    // Compressed file size alone does not establish a credible Gaussian count.
    for name in meta
        .means
        .files
        .iter()
        .chain(meta.quats.files.iter().take(1))
        .chain(meta.scales.files.iter().take(1))
        .chain(meta.sh0.files.iter().take(1))
        .chain(
            meta.sh_n
                .iter()
                .filter(|sh| sh.bands > 0)
                .flat_map(|sh| sh.files.iter().skip(1).take(1)),
        )
    {
        let index = (0..archive.len())
            .find(|&i| {
                archive
                    .by_index_raw(i)
                    .is_ok_and(|entry| entry.name().rsplit('/').next() == Some(name.as_str()))
            })
            .ok_or_else(|| format!("SOG is missing texture {name}"))?;
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        let (width, height) =
            image::ImageReader::with_format(Cursor::new(data), image::ImageFormat::WebP)
                .into_dimensions()
                .map_err(|e| format!("Invalid SOG texture {name}: {e}"))?;
        let pixels = u64::from(width) * u64::from(height);
        if pixels < count as u64 {
            return Err(format!(
                "SOG texture {name} has {pixels} pixels; count requires {count}"
            ));
        }
    }
    Ok(count)
}
