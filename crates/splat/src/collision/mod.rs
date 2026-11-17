//! Sparse voxel collision published next to SuperSplat scenes.
//!
//! A scene ships `scene.voxel.json` (grid metadata) and `scene.voxel.bin`
//! (little-endian `u32` octree nodes followed by leaf masks). The octree
//! addresses `leafSize`^3 voxel blocks; each node is either the solid sentinel,
//! a mixed leaf (`childMask == 0`, low 24 bits index a 64-bit voxel mask), or an
//! interior node (`childMask` in the top byte, first child index in the low 24
//! bits, children packed in octant order).
use bevy::prelude::*;

#[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
mod overlay;
#[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
pub use overlay::VoxelOverlayPlugin;

/// Node value marking a fully solid subtree.
pub const SOLID: u32 = 0xFF00_0000;
const INDEX: u32 = 0x00FF_FFFF;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    #[serde(default)]
    version: Option<String>,
    grid_bounds: Bounds,
    voxel_resolution: f32,
    leaf_size: u32,
    tree_depth: u32,
    node_count: usize,
    leaf_data_count: usize,
}

#[derive(serde::Deserialize)]
struct Bounds {
    min: [f32; 3],
    max: [f32; 3],
}

/// Main-world overlay request: what to draw and which entity carries its frame.
/// Drawn by `VoxelOverlayPlugin`, which needs storage buffers (not WebGL2).
#[derive(Resource, Clone, Default)]
pub struct VoxelOverlay {
    /// Collision voxels for the current scene, in its source coordinates.
    pub collision: Option<std::sync::Arc<VoxelCollision>>,
    /// Entity whose global transform maps source coordinates to the world,
    /// normally the splat root. Without one the grid is drawn untransformed.
    pub root: Option<Entity>,
    /// Draw the overlay this frame.
    pub enabled: bool,
}

/// Decoded voxel octree in the scene's source coordinates.
#[derive(Clone, Debug)]
pub struct VoxelCollision {
    /// Minimum grid corner.
    pub grid_min: Vec3,
    /// Voxel count along each axis.
    pub voxels: UVec3,
    /// Voxel edge length.
    pub resolution: f32,
    /// Voxels per block edge (the leaf mask covers `leaf_size`^3 voxels).
    pub leaf_size: u32,
    /// Octree levels above the block level.
    pub tree_depth: u32,
    /// Octree nodes.
    pub nodes: Vec<u32>,
    /// Two words (low, high) per mixed leaf.
    pub leaf_data: Vec<u32>,
    /// Files before version 1.1 are in the splats' source coordinates; later
    /// ones are in SuperSplat's world, the source turned half a turn about Z.
    pub legacy_frame: bool,
}

impl VoxelCollision {
    /// Decode metadata JSON and its binary, which may still be gzip-encoded
    /// when the HTTP client did not apply the server's `Content-Encoding`.
    pub fn parse(metadata: &[u8], binary: &[u8]) -> Result<Self, String> {
        let meta: Metadata = serde_json::from_slice(metadata)
            .map_err(|error| format!("Invalid voxel metadata: {error}"))?;
        let binary = gunzip(binary)?;
        if binary.len() % 4 != 0 {
            return Err("Voxel binary is not a whole number of u32 words".into());
        }
        let words = binary.len() / 4;
        let expected = meta.node_count + meta.leaf_data_count;
        if words < expected {
            return Err(format!(
                "Voxel binary has {words} words; metadata expects {expected}"
            ));
        }
        if meta.leaf_size != 4 {
            return Err(format!(
                "Unsupported voxel leaf size {} (expected 4)",
                meta.leaf_size
            ));
        }
        if !(1..=24).contains(&meta.tree_depth) {
            return Err(format!("Unsupported voxel tree depth {}", meta.tree_depth));
        }
        if !(meta.voxel_resolution.is_finite() && meta.voxel_resolution > 0.0) {
            return Err("Voxel resolution must be positive".into());
        }
        let mut all = binary.as_chunks::<4>().0[..expected]
            .iter()
            .map(|word| u32::from_le_bytes(*word));
        let nodes: Vec<u32> = all.by_ref().take(meta.node_count).collect();
        let leaf_data: Vec<u32> = all.collect();
        let min = Vec3::from_array(meta.grid_bounds.min);
        let max = Vec3::from_array(meta.grid_bounds.max);
        let voxels = ((max - min) / meta.voxel_resolution)
            .round()
            .max(Vec3::ZERO);
        let legacy_frame = meta
            .version
            .as_deref()
            .and_then(|version| version.parse::<f32>().ok())
            .is_none_or(|version| version < 1.1);
        Ok(Self {
            grid_min: min,
            voxels: voxels.as_uvec3(),
            resolution: meta.voxel_resolution,
            leaf_size: meta.leaf_size,
            tree_depth: meta.tree_depth,
            nodes,
            leaf_data,
            legacy_frame,
        })
    }

    /// Transform from the splats' source coordinates to the grid's storage frame.
    pub fn grid_from_source(&self) -> Mat4 {
        if self.legacy_frame {
            Mat4::IDENTITY
        } else {
            Mat4::from_scale(Vec3::new(-1.0, -1.0, 1.0))
        }
    }

    /// Whether voxel `(x, y, z)` is solid; outside the grid is free space.
    pub fn is_solid(&self, voxel: IVec3) -> bool {
        if self.nodes.is_empty()
            || voxel.cmplt(IVec3::ZERO).any()
            || voxel.as_uvec3().cmpge(self.voxels).any()
        {
            return false;
        }
        let voxel = voxel.as_uvec3();
        let block = voxel / self.leaf_size;
        let mut index = 0usize;
        for level in (0..self.tree_depth).rev() {
            let Some(&node) = self.nodes.get(index) else {
                return false;
            };
            if node == SOLID {
                return true;
            }
            let mask = node >> 24;
            if mask == 0 {
                return self.leaf_bit(node, voxel);
            }
            let bits = (block >> level) & 1;
            let octant = (bits.z << 2) | (bits.y << 1) | bits.x;
            if mask & (1 << octant) == 0 {
                return false;
            }
            index = ((node & INDEX) + (mask & ((1 << octant) - 1)).count_ones()) as usize;
        }
        match self.nodes.get(index) {
            Some(&SOLID) => true,
            Some(&node) => self.leaf_bit(node, voxel),
            None => false,
        }
    }

    /// Whether the voxel containing grid-frame point `point` is solid.
    pub fn is_solid_at(&self, point: Vec3) -> bool {
        self.is_solid(
            ((point - self.grid_min) / self.resolution)
                .floor()
                .as_ivec3(),
        )
    }

    fn leaf_bit(&self, node: u32, voxel: UVec3) -> bool {
        let leaf = (node & INDEX) as usize;
        let local = voxel & 3;
        let bit = local.z * 16 + local.y * 4 + local.x;
        let word = self.leaf_data.get(leaf * 2 + (bit / 32) as usize);
        word.is_some_and(|word| (word >> (bit % 32)) & 1 == 1)
    }
}

fn gunzip(bytes: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, String> {
    use std::io::Read;
    if !bytes.starts_with(&[0x1f, 0x8b]) {
        return Ok(bytes.into());
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|error| format!("Could not decompress voxel binary: {error}"))?;
    Ok(out.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Depth-1 tree over 2x1x1 blocks: block 0 solid, block 1 a mixed leaf.
    fn sample(version: &str, mask: u64) -> (Vec<u8>, Vec<u8>) {
        let metadata = format!(
            r#"{{"version":"{version}","gridBounds":{{"min":[0,0,0],"max":[0.8,0.4,0.4]}},
            "voxelResolution":0.1,"leafSize":4,"treeDepth":1,"nodeCount":3,"leafDataCount":2}}"#
        );
        let words = [0b11 << 24 | 1, SOLID, 0, mask as u32, (mask >> 32) as u32];
        (
            metadata.into_bytes(),
            words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        )
    }

    #[test]
    fn decodes_solid_mixed_and_empty_space() {
        // Mixed leaf: only local voxel (1, 2, 3) is solid (bit 3*16 + 2*4 + 1 = 57).
        let (metadata, binary) = sample("1.1", 1 << 57);
        let voxels = VoxelCollision::parse(&metadata, &binary).unwrap();
        assert_eq!(voxels.voxels, UVec3::new(8, 4, 4));
        assert!(!voxels.legacy_frame);
        // Measured on SuperSplat 8429e5e2: 98% of splat centers fall inside
        // the grid with X/Y negated, 16% without.
        assert_eq!(
            voxels
                .grid_from_source()
                .transform_point3(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(-1.0, -2.0, 3.0)
        );
        assert!(voxels.is_solid(IVec3::new(0, 0, 0)));
        assert!(voxels.is_solid(IVec3::new(3, 3, 3)));
        assert!(voxels.is_solid(IVec3::new(5, 2, 3)));
        assert!(!voxels.is_solid(IVec3::new(4, 2, 3)));
        assert!(!voxels.is_solid(IVec3::new(8, 0, 0)));
        assert!(!voxels.is_solid(IVec3::new(-1, 0, 0)));
        assert!(voxels.is_solid_at(Vec3::new(0.55, 0.25, 0.35)));
    }

    #[test]
    fn accepts_gzip_and_keeps_legacy_files_in_source_coordinates() {
        use std::io::Write;
        let (metadata, binary) = sample("1.0", u64::MAX);
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&binary).unwrap();
        let voxels = VoxelCollision::parse(&metadata, &gzip.finish().unwrap()).unwrap();
        assert!(voxels.legacy_frame);
        assert!(voxels.is_solid(IVec3::new(7, 3, 3)));
        assert_eq!(voxels.grid_from_source(), Mat4::IDENTITY);
    }

    #[test]
    fn rejects_truncated_binaries() {
        let (metadata, binary) = sample("1.1", 0);
        assert!(VoxelCollision::parse(&metadata, &binary[..binary.len() - 4]).is_err());
        assert!(VoxelCollision::parse(&metadata, &binary[..binary.len() - 1]).is_err());
    }

    /// Cross-check against a downloaded scene:
    /// `DIMENSIFY_VOXEL_SCENE=/path/scene.voxel.json cargo test ... -- --ignored`.
    #[test]
    #[ignore = "needs a downloaded SuperSplat voxel scene"]
    fn published_scene_matches_its_metadata() {
        let path = std::env::var("DIMENSIFY_VOXEL_SCENE").unwrap();
        let metadata = std::fs::read(&path).unwrap();
        let binary = std::fs::read(path.replace(".voxel.json", ".voxel.bin")).unwrap();
        let voxels = VoxelCollision::parse(&metadata, &binary).unwrap();
        let mut solid = 0usize;
        for z in 0..voxels.voxels.z as i32 {
            for y in 0..voxels.voxels.y as i32 {
                for x in 0..voxels.voxels.x as i32 {
                    solid += voxels.is_solid(IVec3::new(x, y, z)) as usize;
                }
            }
        }
        let mixed: u32 = voxels.leaf_data.iter().map(|w| w.count_ones()).sum();
        // Every set leaf bit inside the grid is reachable through the tree.
        assert!(solid >= mixed as usize, "{solid} < {mixed}");
        println!("solid voxels: {solid}");
    }
}
