//! Streamed SOG datasets: a `lod-meta.json` index over unbundled SOG chunks,
//! stored as a directory or packaged as a `.ssog` zip archive.
//!
//! Spec: <https://developer.playcanvas.com/user-manual/gaussian-splatting/formats/streamed-sog/>
//!
//! Every leaf is loaded at its finest LOD (level 0 when present), plus the
//! environment, into one cloud. Camera-dependent LOD selection and streaming
//! belong to consumers such as `dimensify_splat`.

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;
use sog_codec::expanded::Expanded;

use super::sog::{SogError, decode_expanded_with, from_expanded, read_file};
use crate::gaussian::formats::planar_3d::PlanarGaussian3d;

#[derive(Deserialize)]
struct LodMeta {
    /// Pre-versioned datasets omit this field.
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    environment: Option<String>,
    filenames: Vec<String>,
    tree: Node,
}

#[derive(Deserialize)]
struct Node {
    #[serde(default)]
    children: Option<Vec<Node>>,
    #[serde(default)]
    lods: Option<BTreeMap<usize, LodRun>>,
}

#[derive(Clone, Copy, Deserialize)]
struct LodRun {
    /// Index into `filenames`.
    file: usize,
    offset: usize,
    count: usize,
}

fn invalid(reason: impl Into<String>) -> SogError {
    SogError::Field {
        field: "lod-meta.json",
        reason: reason.into(),
    }
}

/// Index paths are relative `.../meta.json` paths that stay inside the dataset.
fn checked_meta_path(path: &str) -> Result<&str, SogError> {
    let valid = !path.starts_with('/')
        && !path.contains([':', '\\', '?', '#'])
        && !path.split('/').any(|part| part == ".." || part.is_empty())
        && path.rsplit('/').next() == Some("meta.json");
    valid
        .then_some(path)
        .ok_or_else(|| invalid(format!("'{path}' is not a relative meta.json path")))
}

fn collect_finest_runs(node: &Node, out: &mut Vec<LodRun>) -> Result<(), SogError> {
    match (&node.children, &node.lods) {
        (Some(children), None) => children
            .iter()
            .try_for_each(|child| collect_finest_runs(child, out)),
        (None, Some(lods)) => {
            out.extend(lods.values().next().copied());
            Ok(())
        }
        _ => Err(invalid("nodes must be either interior or leaf")),
    }
}

/// Splat-major planes copied out of decoded chunks.
struct Planes {
    expanded: Expanded,
    count: usize,
}

impl Planes {
    fn extend(&mut self, chunk: &Expanded, start: usize, end: usize) {
        let harmonics = chunk.harmonics.len() / (chunk.positions.len() / 4);
        let out = &mut self.expanded;
        out.positions
            .extend_from_slice(&chunk.positions[start * 4..end * 4]);
        out.rotations
            .extend_from_slice(&chunk.rotations[start * 4..end * 4]);
        out.scales
            .extend_from_slice(&chunk.scales[start * 4..end * 4]);
        out.harmonics
            .extend_from_slice(&chunk.harmonics[start * harmonics..end * harmonics]);
        self.count += end - start;
    }

    /// Pad to a multiple of 32 like the PLY loader and single-scene decoder.
    fn finish(mut self, harmonics: usize) -> PlanarGaussian3d {
        let pad = 32 - self.count % 32;
        let out = &mut self.expanded;
        for _ in 0..pad {
            out.positions.extend_from_slice(&[0., 0., 0., 1.]);
        }
        out.rotations.resize(out.positions.len(), 0.);
        out.scales.resize(out.positions.len(), 0.);
        out.harmonics
            .resize(out.positions.len() / 4 * harmonics, 0.);
        from_expanded(self.expanded)
    }
}

/// Decode a streamed SOG dataset. `read` returns the bytes of a file named
/// relative to the dataset root, for example `lod-meta.json` or `0_0/means_l.webp`.
pub fn decode_streamed(
    mut read: impl FnMut(&str) -> Result<Vec<u8>, SogError>,
) -> Result<PlanarGaussian3d, SogError> {
    let index: LodMeta = serde_json::from_slice(&read("lod-meta.json")?)
        .map_err(|error| invalid(error.to_string()))?;
    if index.version.is_some_and(|version| version != 1) {
        return Err(SogError::Unsupported("streamed SOG version must be 1"));
    }

    let mut runs = Vec::new();
    collect_finest_runs(&index.tree, &mut runs)?;

    let mut decode_chunk = |meta_path: &str| {
        let meta_path = checked_meta_path(meta_path)?;
        let prefix = meta_path.strip_suffix("meta.json").unwrap_or_default();
        decode_expanded_with(|name| read(&format!("{prefix}{name}")))
    };

    // Decode each chunk on first use and drop it after its last run, so only
    // chunks still referenced are held in memory.
    let mut remaining: HashMap<usize, usize> = HashMap::new();
    for run in &runs {
        *remaining.entry(run.file).or_default() += 1;
    }
    let mut chunks: HashMap<usize, (Expanded, usize)> = HashMap::new();
    let mut planes = Planes {
        expanded: Expanded {
            positions: Vec::new(),
            rotations: Vec::new(),
            scales: Vec::new(),
            harmonics: Vec::new(),
        },
        count: 0,
    };
    let mut harmonics = None;

    for run in runs {
        if !chunks.contains_key(&run.file) {
            let path = index
                .filenames
                .get(run.file)
                .ok_or_else(|| invalid("run references a missing chunk"))?;
            chunks.insert(run.file, decode_chunk(path)?);
        }
        let (chunk, count) = &chunks[&run.file];
        let end = run
            .offset
            .checked_add(run.count)
            .filter(|&end| end <= *count)
            .ok_or_else(|| invalid("run exceeds its chunk"))?;
        harmonics.get_or_insert(chunk.harmonics.len() / (chunk.positions.len() / 4));
        planes.extend(chunk, run.offset, end);

        let left = remaining.get_mut(&run.file).expect("counted above");
        *left -= 1;
        if *left == 0 {
            chunks.remove(&run.file);
        }
    }

    if let Some(environment) = &index.environment {
        let (chunk, count) = decode_chunk(environment)?;
        harmonics.get_or_insert(chunk.harmonics.len() / (chunk.positions.len() / 4));
        planes.extend(&chunk, 0, count);
    }

    let harmonics = harmonics.ok_or(SogError::Unsupported("streamed SOG has no splats"))?;
    Ok(planes.finish(harmonics))
}

/// Decode a streamed dataset packaged as a zip (`.ssog`). The dataset root is
/// the directory holding `lod-meta.json`, which may be nested in the archive.
pub fn decode_streamed_bundle(bytes: &[u8]) -> Result<PlanarGaussian3d, SogError> {
    use std::io::Read;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let root = archive
        .file_names()
        .filter_map(|name| name.strip_suffix("lod-meta.json"))
        .filter(|root| root.is_empty() || root.ends_with('/'))
        .min_by_key(|root| root.len())
        .ok_or(SogError::MissingFile("lod-meta.json".into()))?
        .to_owned();

    decode_streamed(|name| {
        let mut entry = archive
            .by_name(&format!("{root}{name}"))
            .map_err(|_| SogError::MissingFile(name.to_owned()))?;
        let mut contents = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut contents)
            .map_err(|source| SogError::ZipEntry {
                file: name.to_owned(),
                source,
            })?;
        Ok(contents)
    })
}

/// Decode a streamed dataset stored as a directory holding `lod-meta.json`.
pub fn decode_streamed_dir(dir: &std::path::Path) -> Result<PlanarGaussian3d, SogError> {
    decode_streamed(|name| read_file(&dir.join(name), name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sog::tests::{fixture, meta_json};

    /// Two chunks of the 3-splat fixture. Leaf 0 takes splat 0 of chunk 0 at
    /// LOD 0; leaf 1 has only LOD 1, taking splats 1..3 of chunk 1.
    fn dataset(environment: bool) -> Vec<(String, Vec<u8>)> {
        let index = serde_json::json!({
            "version": 1,
            "lodLevels": 2,
            "filenames": ["0_0/meta.json", "1_1/meta.json"],
            "environment": environment.then_some("env/meta.json"),
            "tree": {
                "bound": {"min": [-3, -3, -3], "max": [3, 3, 3]},
                "children": [
                    {
                        "bound": {"min": [-3, -3, -3], "max": [3, 3, 3]},
                        "lods": {
                            "0": {"file": 0, "offset": 0, "count": 1},
                            "1": {"file": 1, "offset": 0, "count": 1}
                        }
                    },
                    {
                        "bound": {"min": [-3, -3, -3], "max": [3, 3, 3]},
                        "lods": {"1": {"file": 1, "offset": 1, "count": 2}}
                    }
                ]
            }
        });
        let mut files = vec![("lod-meta.json".to_owned(), index.to_string().into_bytes())];
        let (_, scene) = fixture(3, false);
        for dir in ["0_0", "1_1", "env"] {
            files.push((format!("{dir}/meta.json"), meta_json(3, false).into_bytes()));
            for (name, bytes) in &scene {
                files.push((format!("{dir}/{name}"), bytes.clone()));
            }
        }
        files
    }

    fn zip(files: &[(String, Vec<u8>)], root: &str) -> Vec<u8> {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            writer.start_file(format!("{root}{name}"), options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn positions(cloud: &PlanarGaussian3d, count: usize) -> Vec<[f32; 3]> {
        cloud.position_visibility[..count]
            .iter()
            .map(|p| p.position)
            .collect()
    }

    #[test]
    fn finest_lod_runs_and_environment_merge_into_one_padded_cloud() {
        let files: HashMap<_, _> = dataset(true).into_iter().collect();
        let cloud = decode_streamed(|name| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| SogError::MissingFile(name.to_owned()))
        })
        .unwrap();

        let (meta, scene) = fixture(3, false);
        let single = crate::io::sog::decode_sog(&meta, &scene).unwrap();
        let splat = |i: usize| single.position_visibility[i].position;
        // Leaf 0 at LOD 0 (chunk 0, splat 0); leaf 1 at its only LOD (chunk 1,
        // splats 1..3); then all 3 environment splats.
        assert_eq!(
            positions(&cloud, 6),
            [splat(0), splat(1), splat(2), splat(0), splat(1), splat(2)]
        );
        assert_eq!(cloud.position_visibility.len(), 32);
        assert_eq!(cloud.rotation.len(), 32);
        assert_eq!(cloud.spherical_harmonic.len(), 32);
        assert_eq!(cloud.scale_opacity[6].scale, [0.; 3]);
    }

    #[test]
    fn nested_ssog_archives_decode_through_the_archive_dispatch() {
        let flat = crate::io::sog::decode_sog_archive(&zip(&dataset(false), "")).unwrap();
        let nested = crate::io::sog::decode_sog_archive(&zip(&dataset(false), "scene/")).unwrap();
        assert_eq!(positions(&flat, 3), positions(&nested, 3));
    }

    #[test]
    fn directories_decode_like_archives() {
        let dir = std::env::temp_dir().join(format!("ssog-dir-{}", std::process::id()));
        for (name, bytes) in dataset(false) {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let from_dir = decode_streamed_dir(&dir);
        let single = crate::io::sog::decode_sog_dir(&dir.join("0_0"));
        std::fs::remove_dir_all(&dir).unwrap();

        let archive = decode_streamed_bundle(&zip(&dataset(false), "")).unwrap();
        assert_eq!(positions(&from_dir.unwrap(), 3), positions(&archive, 3));
        assert_eq!(single.unwrap().position_visibility.len(), 32);
    }

    /// Decode the dataset after editing its index.
    fn decode_edited(edit: impl FnOnce(&mut serde_json::Value)) -> Result<(), SogError> {
        let mut files: HashMap<_, _> = dataset(false).into_iter().collect();
        let mut index: serde_json::Value = serde_json::from_slice(&files["lod-meta.json"]).unwrap();
        edit(&mut index);
        files.insert("lod-meta.json".into(), index.to_string().into_bytes());
        decode_streamed(|name| {
            files
                .get(name)
                .cloned()
                .ok_or_else(|| SogError::MissingFile(name.to_owned()))
        })
        .map(|_| ())
    }

    #[test]
    fn escaping_paths_and_oversized_runs_are_rejected() {
        for edit in [
            |index: &mut serde_json::Value| index["filenames"][0] = "../0_0/meta.json".into(),
            |index: &mut serde_json::Value| index["filenames"][0] = "/0_0/meta.json".into(),
            |index: &mut serde_json::Value| {
                index["tree"]["children"][1]["lods"]["1"]["offset"] = 2.into()
            },
        ] {
            assert!(matches!(
                decode_edited(edit),
                Err(SogError::Field {
                    field: "lod-meta.json",
                    ..
                })
            ));
        }
    }

    #[test]
    fn unsupported_versions_are_rejected() {
        assert!(matches!(
            decode_edited(|index| index["version"] = 2.into()),
            Err(SogError::Unsupported(_))
        ));
    }
}
