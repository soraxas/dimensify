//! Export public deterministic assets for real browser and native replay.
use dimensify_perf::{
    SogFixture,
    pipeline::{binary_ply, decode_packed, sog_bundle},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

fn export(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Refuse existing destinations before generating or changing any assets.
    std::fs::create_dir(output)?;
    let fixture = SogFixture::new();
    let mut files = BTreeMap::new();
    files.insert("fixture-10000.ply".to_owned(), binary_ply(10_000));
    files.insert("fixture-100000.ply".to_owned(), binary_ply(100_000));
    files.insert("fixture-65536.sog".to_owned(), sog_bundle(&fixture));

    let mut coarse = SogFixture {
        meta: fixture.meta.clone(),
        files: fixture.files.clone(),
    };
    coarse.meta.count = 16_384;
    for data in coarse.files.values_mut() {
        let rgba = image::load_from_memory(data)?.to_rgba8();
        if rgba.dimensions() == (256, 256) {
            // Preserve scene coverage by sampling across the whole fine source.
            let sampled: Vec<_> = rgba
                .as_raw()
                .as_chunks::<4>()
                .0
                .iter()
                .step_by(4)
                .flatten()
                .copied()
                .collect();
            let mut encoded = Vec::new();
            image::codecs::webp::WebPEncoder::new_lossless(&mut encoded).encode(
                &sampled,
                128,
                128,
                image::ExtendedColorType::Rgba8,
            )?;
            *data = encoded;
        }
    }
    for (lod, source) in [("fine", &fixture), ("coarse", &coarse)] {
        // Use the same admission envelope as production chunk decoding.
        assert_eq!(decode_packed(source).count() as usize, source.meta.count);
        files.insert(
            format!("stream/{lod}/meta.json"),
            serde_json::to_vec(&source.meta)?,
        );
        for (name, data) in &source.files {
            files.insert(format!("stream/{lod}/{name}"), data.clone());
        }
    }
    let manifest = serde_json::json!({
        "version": 1, "lodLevels": 2,
        "filenames": ["fine/meta.json", "coarse/meta.json"],
        "tree": {"bound": {"min": [-1.719, -1.719, -1.719], "max": [1.719, 1.719, 1.719]},
            "lods": {"0": {"file": 0, "offset": 0, "count": 65_536},
                     "1": {"file": 1, "offset": 0, "count": 16_384}}}
    });
    let manifest = serde_json::to_vec(&manifest)?;
    dimensify_splat::streaming::Manifest::parse(&manifest).unwrap();
    files.insert("stream/lod-meta.json".into(), manifest);
    let records: Vec<_> = files
        .iter()
        .map(|(path, bytes)| {
            serde_json::json!({
                "path": path, "bytes": bytes.len(), "sha256": format!("{:x}", Sha256::digest(bytes))
            })
        })
        .collect();
    for (path, bytes) in files {
        let path = output.join(path);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    let metadata = serde_json::json!({
        "schema": 1, "synthetic": true,
        "description": "Deterministic loader fixtures; not a representative production capture",
        "assets": [
            {"path": "fixture-10000.ply", "count": 10_000},
            {"path": "fixture-100000.ply", "count": 100_000},
            {"path": "fixture-65536.sog", "count": 65_536},
            {"path": "stream/lod-meta.json", "count": 65_536, "coarse_count": 16_384}
        ], "files": records
    });
    std::fs::write(
        output.join("fixtures.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "--output" {
        eprintln!("usage: perf-fixtures --output NEW_DIRECTORY");
        std::process::exit(2);
    }
    let output = Path::new(&args[1]);
    if let Err(error) = export(output) {
        eprintln!("fixture export failed: {error}");
        std::process::exit(1);
    }
    println!(
        "Generated fixtures and SHA256 metadata in {}",
        output.display()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use dimensify_splat::{decode_splat, streaming::Manifest};

    #[test]
    fn exported_assets_decode_and_stream_manifest_keeps_both_lods() {
        let output =
            std::env::temp_dir().join(format!("dimensify-perf-fixtures-{}", std::process::id()));
        export(&output).unwrap();
        for (name, count) in [
            ("fixture-10000.ply", 10_000),
            ("fixture-100000.ply", 100_000),
            ("fixture-65536.sog", 65_536),
        ] {
            let bytes = std::fs::read(output.join(name)).unwrap();
            assert_eq!(decode_splat(name, &bytes).unwrap().count, count);
        }
        let manifest =
            Manifest::parse(&std::fs::read(output.join("stream/lod-meta.json")).unwrap()).unwrap();
        assert_eq!(manifest.file_counts, [65_536, 16_384]);
        assert_eq!(manifest.coarse_files(), [1]);
        assert_eq!(manifest.leaves[0].lods.len(), 2);
        for (lod, count, dimensions) in
            [("fine", 65_536, [256, 256]), ("coarse", 16_384, [128, 128])]
        {
            let directory = output.join("stream").join(lod);
            let meta = sog_codec::expanded::parse_meta(
                &std::fs::read(directory.join("meta.json")).unwrap(),
            )
            .unwrap();
            let files = meta
                .referenced_files()
                .into_iter()
                .map(|name| {
                    (
                        name.to_owned(),
                        std::fs::read(directory.join(name)).unwrap(),
                    )
                })
                .collect();
            let fixture = dimensify_perf::SogFixture { meta, files };
            let packed = dimensify_perf::pipeline::decode_packed(&fixture);
            assert_eq!(packed.count(), count);
            assert_eq!(packed.means_lo().dimensions(), dimensions);
            if lod == "coarse" {
                assert_eq!(packed.means_lo().texels()[4..8], [4, 0, 0, 255]);
                assert_eq!(packed.means_hi().texels()[(16_384 - 1) * 4], 255);
            }
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("fixtures.json")).unwrap()).unwrap();
        assert_eq!(metadata["synthetic"], true);
        assert_eq!(metadata["assets"][0]["count"], 10_000);
        assert_eq!(metadata["assets"][2]["count"], 65_536);
        assert_eq!(metadata["files"].as_array().unwrap().len(), 20);
        assert!(
            metadata["files"]
                .as_array()
                .unwrap()
                .iter()
                .all(|file| file["sha256"].as_str().unwrap().len() == 64)
        );
        let before = std::fs::read(output.join("fixtures.json")).unwrap();
        assert!(export(&output).is_err());
        assert_eq!(std::fs::read(output.join("fixtures.json")).unwrap(), before);
        std::fs::remove_dir_all(output).unwrap();
    }
}
