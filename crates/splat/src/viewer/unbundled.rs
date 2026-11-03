//! Adapt a single-resolution SOG dataset to the existing source/readiness pipeline.
use crate::assets::{AssetLocation, AssetResolver, url_directory};
use crate::streaming::runtime::SogStream;
use bevy::prelude::*;
use bevy_gaussian_splatting::io::sog;

pub(super) fn stream(bytes: &[u8], url: &str, name: String) -> Result<SogStream, String> {
    if bytes.len() > 1024 * 1024 {
        return Err("SOG metadata exceeds the 1 MiB limit".into());
    }
    let meta = sog::parse_meta(bytes).map_err(|error| error.to_string())?;
    if meta.count == 0 || meta.count > u32::MAX as usize {
        return Err("SOG count must be nonzero and fit the renderer index range".into());
    }
    if meta.quats.files.len() != 1 || meta.scales.files.len() != 1 || meta.sh0.files.len() != 1 {
        return Err("SOG rotation, scale and color planes each require exactly one texture".into());
    }
    let directory = url::Url::parse(url_directory(url)).map_err(|_| "Invalid SOG metadata URL")?;
    if !matches!(directory.scheme(), "http" | "https") || directory.host_str().is_none() {
        return Err("SOG metadata requires an HTTP(S) URL".into());
    }
    // Existing chunk transport resolves dataset files relative to the metadata
    // directory. Keep them relative and never propagate the manifest's query.
    if meta.referenced_files().iter().any(|path| {
        let normalized = path.to_ascii_lowercase().replace("%2e", ".");
        path.is_empty()
            || path.starts_with('/')
            || path.contains([':', '\\', '?', '#'])
            || normalized
                .split('/')
                .any(|part| part == ".." || part.is_empty())
            || path.to_ascii_lowercase().contains("%2f")
            || path.to_ascii_lowercase().contains("%5c")
            || !directory.join(path).is_ok_and(|resolved| {
                resolved.origin() == directory.origin()
                    && resolved.path().starts_with(directory.path())
            })
    }) {
        return Err("SOG textures must use relative filenames without query parameters".into());
    }
    // Metadata stores signed-log position bounds, not world-space coordinates.
    // Match ordinary file framing by fitting Gaussian centers.
    let inverse = |x: f64| (x.signum() * x.abs().exp_m1()) as f32;
    let min = Vec3::from_array(meta.means.mins.map(inverse));
    let max = Vec3::from_array(meta.means.maxs.map(inverse));
    if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() {
        return Err("SOG position bounds must be finite and ordered".into());
    }
    let hierarchy = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "lodLevels": 1,
        "filenames": ["meta.json"],
        "tree": {
            "bound": {"min": min.to_array(), "max": max.to_array()},
            "lods": {"0": {"file": 0, "offset": 0, "count": meta.count}}
        }
    }))
    .map_err(|error| error.to_string())?;
    let metadata_reference = format!("{}meta.json", url_directory(url));
    let metadata: std::sync::Arc<[u8]> = bytes.into();
    let resolver = AssetResolver::new(move |reference| {
        if reference == metadata_reference {
            return Ok(AssetLocation::Bytes(metadata.clone()));
        }
        let url = url::Url::parse(reference).map_err(|_| "Invalid SOG texture URL")?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("SOG textures require HTTP(S) URLs".into());
        }
        Ok(AssetLocation::Url(url.into()))
    });
    // One leaf, one source, one resolution. No refinement levels are invented.
    SogStream::parse(&hierarchy, url, name, "viewer-stream".into())
        .map(|stream| stream.with_resolver(resolver))
}

#[cfg(test)]
mod tests {
    use super::*;
    const META: &[u8] = include_bytes!("../../tests/fixtures/hosted-stream/chunks/meta.json");

    #[test]
    fn framing_inverts_signed_log_bounds_and_keeps_source_coordinates() {
        let stream = stream(META, "https://assets.test/data?key=private", "scene".into()).unwrap();
        let extent = stream.fit_extent();
        assert!(extent.center.length() < 1e-5);
        assert!((extent.half_extents - Vec3::ONE).length() < 1e-5);
    }

    #[test]
    fn invalid_metadata_is_rejected_before_texture_requests() {
        let original: serde_json::Value = serde_json::from_slice(META).unwrap();
        for (pointer, value) in [
            ("/version", serde_json::json!(3)),
            ("/count", serde_json::json!(0)),
            ("/means/mins/0", serde_json::json!(1000.0)),
            ("/means/mins/0", serde_json::json!(2.0)),
            ("/means/files/0", serde_json::json!("../private.webp")),
            ("/means/files/0", serde_json::json!("%2e%2e/private.webp")),
            (
                "/quats/files",
                serde_json::json!(["quats.webp", "extra.webp"]),
            ),
            (
                "/means/files/0",
                serde_json::json!("https://other.test/file.webp"),
            ),
            (
                "/means/files/0",
                serde_json::json!("file.webp?token=private"),
            ),
        ] {
            let mut metadata = original.clone();
            *metadata.pointer_mut(pointer).unwrap() = value;
            assert!(
                stream(
                    &serde_json::to_vec(&metadata).unwrap(),
                    "https://assets.test/meta.json",
                    "scene".into()
                )
                .is_err()
            );
        }
    }
}
