#[cfg(any(feature = "reqwest", feature = "ehttp"))]
pub mod http;

#[cfg(feature = "fs")]
pub mod fs;

#[cfg(any(feature = "reqwest", feature = "ehttp", feature = "fs"))]
pub mod url;

use bytes::Bytes;

use crate::{metadata::Metadata, utils::ConditionalSendFuture};

pub trait PotreeAsset: Sync + Send {
    type Error: std::error::Error + Sync + Send + 'static;

    fn read_metadata(&self) -> impl ConditionalSendFuture<Output = Result<Bytes, Self::Error>>;

    fn read_hierarchy(
        &self,
        offset: u64,
        length: usize,
    ) -> impl ConditionalSendFuture<Output = Result<Bytes, Self::Error>>;

    fn read_octree(
        &self,
        offset: u64,
        length: usize,
    ) -> impl ConditionalSendFuture<Output = Result<Bytes, Self::Error>>;

    /// Read a single octree node's point-data bytes, addressed by its
    /// Potree-v1-style path name (e.g. `"r0263"`) in addition to the
    /// `byte_offset`/`byte_size` coordinates into a monolithic `octree.bin`.
    ///
    /// Backends that store nodes as independent files (the "multi-file"
    /// layout) can fetch by `name` directly, with a plain GET/read — no
    /// Range request into a shared resource, which is what makes per-node
    /// files cache reliably in the browser under concurrent loads. The
    /// default implementation preserves today's single-file behavior for
    /// every existing [`PotreeAsset`] implementation.
    fn read_octree_node(
        &self,
        _name: &str,
        offset: u64,
        length: usize,
    ) -> impl ConditionalSendFuture<Output = Result<Bytes, Self::Error>> {
        self.read_octree(offset, length)
    }
}

/// Which octree node storage layout a dataset uses, from `metadata.json`'s
/// `octreeLayout` field. Shared by every [`PotreeAsset`] implementation in
/// this module that wants to dispatch [`PotreeAsset::read_octree_node`]
/// between a single `octree.bin` and per-node `octree/<name>.bin` files.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OctreeLayout {
    SingleFile,
    MultiFile,
}

impl OctreeLayout {
    pub fn from_metadata(metadata: &Metadata) -> Self {
        if metadata.octree_layout.as_deref() == Some("multi-file") {
            OctreeLayout::MultiFile
        } else {
            OctreeLayout::SingleFile
        }
    }
}
