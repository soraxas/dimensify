use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    asset::PotreeAsset,
    hierarchy::{Hierarchy, HierarchyAsync, PotreeHierarchyError},
    metadata::Metadata,
};
use bytes::Bytes;
use thiserror::Error;

/// Errors raised while creating a multi-file view of a stock Potree v2 dataset.
#[derive(Debug, Error)]
pub enum RepackError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid metadata.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to read Potree hierarchy: {0}")]
    Hierarchy(#[from] PotreeHierarchyError),
    #[error("source dataset is already marked as multi-file")]
    AlreadyMultiFile,
    #[error("output directory already exists: {0}")]
    OutputExists(PathBuf),
}

struct SingleFileRepackAsset {
    source: PathBuf,
}

impl PotreeAsset for SingleFileRepackAsset {
    type Error = std::io::Error;

    async fn read_metadata(&self) -> Result<Bytes, Self::Error> {
        fs::read(self.source.join("metadata.json")).map(Bytes::from)
    }

    async fn read_hierarchy(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        read_exact_range(&self.source.join("hierarchy.bin"), offset, length).map(Bytes::from)
    }

    async fn read_octree(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        read_exact_range(&self.source.join("octree.bin"), offset, length).map(Bytes::from)
    }
}

fn read_exact_range(path: &Path, offset: u64, length: usize) -> Result<Vec<u8>, std::io::Error> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Copy a stock Potree v2 dataset into the optional per-node-file layout.
///
/// Point data is never decoded or re-encoded: every generated node file is an
/// exact slice of the source `octree.bin`. The source directory is read-only,
/// and the completed output is renamed into place only after every node has
/// been fully written.
pub fn repack_single_file_to_multi_file(
    source: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<(), RepackError> {
    let source = source.as_ref();
    let output = output.as_ref();
    if output.exists() {
        return Err(RepackError::OutputExists(output.to_path_buf()));
    }

    let metadata_bytes = fs::read(source.join("metadata.json"))?;
    let metadata: Metadata = serde_json::from_slice(&metadata_bytes)?;
    if metadata.octree_layout.as_deref() == Some("multi-file") {
        return Err(RepackError::AlreadyMultiFile);
    }

    let nodes = futures::executor::block_on(async {
        let hierarchy = Hierarchy::load(SingleFileRepackAsset {
            source: source.to_path_buf(),
        })
        .await?;
        hierarchy.load_entire_hierarchy().await
    })?;

    let output_parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(output_parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".potree-multi-file-")
        .tempdir_in(output_parent)?;
    let temporary_path = temporary.path();
    fs::create_dir(temporary_path.join("octree"))?;
    fs::copy(
        source.join("hierarchy.bin"),
        temporary_path.join("hierarchy.bin"),
    )?;

    let mut source_octree = File::open(source.join("octree.bin"))?;
    for node in nodes {
        if !node.node_type.has_points() || node.num_points == 0 || node.byte_size == 0 {
            continue;
        }

        source_octree.seek(SeekFrom::Start(node.byte_offset))?;
        let mut bytes = vec![0; node.byte_size as usize];
        source_octree.read_exact(&mut bytes)?;

        let mut node_file = File::create(
            temporary_path
                .join("octree")
                .join(format!("{}.bin", node.name)),
        )?;
        node_file.write_all(&bytes)?;
        node_file.sync_all()?;
    }

    let mut metadata_value: serde_json::Value = serde_json::from_slice(&metadata_bytes)?;
    metadata_value["octreeLayout"] = serde_json::Value::String("multi-file".to_string());
    let mut output_metadata = serde_json::to_vec_pretty(&metadata_value)?;
    output_metadata.push(b'\n');
    fs::write(temporary_path.join("metadata.json"), output_metadata)?;

    let completed = temporary.keep();
    fs::rename(completed, output)?;
    Ok(())
}
