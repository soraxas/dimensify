//! PlayCanvas streamed SOG v1 and pre-versioned manifests.
//! https://developer.playcanvas.com/user-manual/gaussian-splatting/formats/streamed-sog/

use bevy::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;

pub use super::SplatRange;

/// Axis-aligned bounds in source coordinates.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Debug)]
pub struct Leaf {
    pub bound: Bounds,
    /// Finest to coarsest; missing levels are legal.
    pub lods: Vec<(usize, SplatRange)>,
}

#[derive(Debug)]
pub struct Manifest {
    /// Authored scene bounds, independent of resident/selected LOD ranges.
    pub bound: Bounds,
    pub filenames: Vec<String>,
    pub environment: Option<String>,
    pub leaves: Vec<Leaf>,
    pub file_counts: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawManifest {
    version: Option<u32>,
    lod_levels: usize,
    environment: Option<String>,
    filenames: Vec<String>,
    tree: Node,
}

#[derive(Deserialize)]
struct Node {
    bound: Bounds,
    children: Option<Vec<Node>>,
    lods: Option<BTreeMap<usize, SplatRange>>,
}

impl Manifest {
    /// Validate a streamed SOG manifest before scheduling its files.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let raw: RawManifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if raw.version.is_some_and(|v| v != 1) || !(1..=32).contains(&raw.lod_levels) {
            return Err("unsupported streamed SOG version or LOD count".into());
        }
        if raw.filenames.is_empty()
            || raw.filenames.iter().any(|p| !valid_path(p))
            || raw.environment.as_ref().is_some_and(|p| !valid_path(p))
        {
            return Err("invalid relative SOG filename".into());
        }
        let mut leaves = Vec::new();
        let bound = raw.tree.bound;
        flatten(raw.tree, raw.lod_levels, raw.filenames.len(), &mut leaves)?;
        let mut ranges = vec![Vec::new(); raw.filenames.len()];
        for leaf in &leaves {
            for &(level, range) in &leaf.lods {
                ranges[range.file].push((range.offset, range.count, level));
            }
        }
        let mut file_counts = Vec::new();
        for mut file in ranges {
            file.sort_unstable();
            let mut end = 0usize;
            let level = file.first().map(|r| r.2);
            for (offset, count, lod) in file {
                if offset != end || Some(lod) != level {
                    return Err(
                        "chunk ranges must be contiguous, non-overlapping and one LOD".into(),
                    );
                }
                end = end.checked_add(count).ok_or("SOG range overflow")?;
            }
            file_counts.push(end);
        }
        // Discard levels dominated by a finer representation with no more splats.
        // This makes fallback counts monotone even in captures with count inversions.
        for leaf in &mut leaves {
            let mut best = usize::MAX;
            leaf.lods.retain(|(_, range)| {
                if range.count >= best {
                    false
                } else {
                    best = range.count;
                    true
                }
            });
        }
        Ok(Self {
            bound,
            filenames: raw.filenames,
            environment: raw.environment,
            leaves,
            file_counts,
        })
    }

    /// Return unique file indices required for initial coarse coverage.
    pub fn coarse_files(&self) -> Vec<usize> {
        let mut files: Vec<_> = self
            .leaves
            .iter()
            .filter_map(|l| l.lods.last().map(|(_, r)| r.file))
            .collect();
        files.sort_unstable();
        files.dedup();
        files
    }
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains([':', '\\', '?', '#'])
        && !path.split('/').any(|part| part == ".." || part.is_empty())
        && path.ends_with("meta.json")
}

fn flatten(node: Node, levels: usize, files: usize, out: &mut Vec<Leaf>) -> Result<(), String> {
    let Bounds { min, max } = node.bound;
    if !min.is_finite() || !max.is_finite() || min.cmpgt(max).any() {
        return Err("invalid LOD bounds".into());
    }
    match (node.children, node.lods) {
        (Some(children), None) if children.len() == 2 => {
            for child in children {
                flatten(child, levels, files, out)?;
            }
        }
        (None, Some(lods)) if !lods.is_empty() => {
            if lods.iter().any(|(level, range)| {
                *level >= levels
                    || range.file >= files
                    || range.offset.checked_add(range.count).is_none()
            }) {
                return Err("invalid LOD range".into());
            }
            out.push(Leaf {
                bound: node.bound,
                lods: lods.into_iter().collect(),
            });
        }
        _ => return Err("LOD node must have two children or nonempty lods".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const SCENE: &[u8] = include_bytes!("fixtures/sanatorium-lod.json");

    #[test]
    fn sparse_levels_and_nonmonotone_counts_keep_the_best_representations() {
        let m = Manifest::parse(
            br#"{
            "version":1,"lodLevels":4,
            "filenames":["fine/meta.json","middle/meta.json","coarse/meta.json"],
            "tree":{"bound":{"min":[0,0,0],"max":[1,1,1]},"lods":{
                "0":{"file":0,"offset":0,"count":8},
                "2":{"file":1,"offset":0,"count":9},
                "3":{"file":2,"offset":0,"count":2}}}
        }"#,
        )
        .unwrap();
        assert_eq!(
            m.leaves[0]
                .lods
                .iter()
                .map(|(level, _)| *level)
                .collect::<Vec<_>>(),
            [0, 3]
        );
        assert_eq!(m.file_counts, [8, 9, 2]);
    }

    #[test]
    fn parses_legacy_scene_hierarchy() {
        let m = Manifest::parse(SCENE).unwrap();
        assert_eq!(m.leaves.len(), 101);
        assert_eq!(m.filenames.len(), 46);
        assert_eq!(m.coarse_files(), [5]);
        assert_eq!(m.file_counts[5], 404090);
        assert_eq!(m.environment.as_deref(), Some("env/meta.json"));
        assert_eq!(
            m.leaves[1].lods.last().unwrap().1,
            SplatRange {
                file: 5,
                offset: 2,
                count: 1
            }
        );
    }

    #[test]
    fn rejects_bad_ranges_and_future_versions_before_downloading() {
        for (field, value) in [
            ("file", serde_json::json!(999)),
            ("offset", serde_json::json!(1)),
        ] {
            let mut m: serde_json::Value = serde_json::from_slice(SCENE).unwrap();
            let mut node = &mut m["tree"];
            while node.get("children").is_some() {
                node = &mut node["children"][0];
            }
            node["lods"]["0"][field] = value;
            assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_err());
        }
        let mut m: serde_json::Value = serde_json::from_slice(SCENE).unwrap();
        m["version"] = 2.into();
        assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_err());
        m["version"] = 1.into();
        m["environment"] = serde_json::Value::Null;
        assert!(Manifest::parse(&serde_json::to_vec(&m).unwrap()).is_ok());
    }
}
