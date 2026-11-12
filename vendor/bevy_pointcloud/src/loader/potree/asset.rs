use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use bytes::Bytes;
use potree::{asset::PotreeAsset, metadata::Metadata};
use thiserror::Error;

use crate::{ByteSource, ByteSourceError, FileSource, HttpSource};

/// How to reach node data for either octree layout, retaining the single-file
/// source for range reads without opening irrelevant native files.
type DirectoryOpener<S> = Arc<dyn Fn(&str) -> Result<S, ByteSourceError> + Send + Sync>;

enum NodeFetch<S: ByteSource> {
    Directory {
        open: DirectoryOpener<S>,
    },
    Http {
        base_url: String,
        suffix: String,
        octree: S,
    },
    Fs {
        base_path: PathBuf,
        octree: OnceLock<FileSource>,
    },
}

/// Which octree storage layout a dataset uses, detected from
/// `metadata.json`'s `octreeLayout` field the first time it's read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OctreeLayout {
    /// All node point data lives in one `octree.bin`, addressed via HTTP
    /// Range requests / byte offsets. This is Potree v2's original layout.
    SingleFile,
    /// Each node's point data is its own file under `octree/<name>.bin`,
    /// fetched with a plain GET (200 response). Avoids Chrome's cache
    /// unreliability for concurrent Range requests sharing one cache key.
    MultiFile,
}

pub struct PotreeAssetSource<S: ByteSource> {
    metadata: S,
    hierarchy: S,
    node_fetch: NodeFetch<S>,
    layout: OnceLock<OctreeLayout>,
}

#[derive(Debug, Error)]
pub enum PotreeAssetSourceError {
    #[error(transparent)]
    ByteSource(#[from] ByteSourceError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl<S: ByteSource> PotreeAssetSource<S> {
    /// Open a local/virtual directory by relative name. Files remain lazy;
    /// single-file point data is ranged and multi-file nodes are read whole.
    pub fn from_directory(
        open: impl Fn(&str) -> Result<S, ByteSourceError> + Send + Sync + 'static,
    ) -> Result<Self, ByteSourceError> {
        let metadata = open("metadata.json")?;
        let hierarchy = open("hierarchy.bin")?;
        Ok(Self {
            metadata,
            hierarchy,
            node_fetch: NodeFetch::Directory {
                open: Arc::new(open),
            },
            layout: OnceLock::new(),
        })
    }
}

impl<S: ByteSource> PotreeAsset for PotreeAssetSource<S> {
    type Error = PotreeAssetSourceError;

    async fn read_metadata(&self) -> Result<Bytes, Self::Error> {
        let buffer = self.metadata.read_to_end(0).await?;

        if let Ok(metadata) = serde_json::from_slice::<Metadata>(&buffer) {
            let layout = if metadata.octree_layout.as_deref() == Some("multi-file") {
                OctreeLayout::MultiFile
            } else {
                OctreeLayout::SingleFile
            };
            if layout == OctreeLayout::SingleFile {
                if let NodeFetch::Fs { base_path, octree } = &self.node_fetch {
                    file_octree(base_path, octree)?;
                }
            }
            // `Hierarchy::load` always calls `read_metadata` exactly once before
            // any `read_octree_node` call, so this is set before it's read.
            let _ = self.layout.set(layout);
        }

        Ok(buffer.into())
    }

    async fn read_hierarchy(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        let buffer = self.hierarchy.read_range(offset, length as u64).await?;
        Ok(buffer.into())
    }

    async fn read_octree(&self, offset: u64, length: usize) -> Result<Bytes, Self::Error> {
        let buffer = match &self.node_fetch {
            NodeFetch::Directory { open } => {
                open("octree.bin")?
                    .read_range(offset, length as u64)
                    .await?
            }
            NodeFetch::Http { octree, .. } => octree.read_range(offset, length as u64).await?,
            NodeFetch::Fs { base_path, octree } => {
                file_octree(base_path, octree)?
                    .read_range(offset, length as u64)
                    .await?
            }
        };
        Ok(buffer.into())
    }

    async fn read_octree_node(
        &self,
        name: &str,
        offset: u64,
        length: usize,
    ) -> Result<Bytes, Self::Error> {
        // Defensive fallback to single-file behavior if this is somehow
        // called before `read_metadata` — never panics.
        match self
            .layout
            .get()
            .copied()
            .unwrap_or(OctreeLayout::SingleFile)
        {
            OctreeLayout::SingleFile => self.read_octree(offset, length).await,
            OctreeLayout::MultiFile => match &self.node_fetch {
                NodeFetch::Directory { open } => Ok(open(&format!("octree/{name}.bin"))?
                    .read_to_end(0)
                    .await?
                    .into()),
                NodeFetch::Http {
                    base_url, suffix, ..
                } => {
                    let url = potree_file_url(base_url, &format!("{name}.bin"), suffix);
                    let bytes = HttpSource::open(&url)?.read_whole_no_range().await?;
                    Ok(bytes.into())
                }
                NodeFetch::Fs { base_path, .. } => {
                    let bytes = FileSource::open(base_path.join(format!("{name}.bin")))?
                        .read_to_end(0)
                        .await?;
                    Ok(bytes.into())
                }
            },
        }
    }
}

impl PotreeAssetSource<FileSource> {
    pub fn from_path(
        path: impl Into<PathBuf>,
    ) -> Result<PotreeAssetSource<FileSource>, ByteSourceError> {
        let path: PathBuf = path.into();

        Ok(PotreeAssetSource {
            metadata: FileSource::open(path.join("metadata.json"))?,
            hierarchy: FileSource::open(path.join("hierarchy.bin"))?,
            node_fetch: NodeFetch::Fs {
                base_path: path.join("octree"),
                octree: OnceLock::new(),
            },
            layout: OnceLock::new(),
        })
    }
}

impl PotreeAssetSource<HttpSource> {
    pub fn from_url(url: &str) -> Result<PotreeAssetSource<HttpSource>, ByteSourceError> {
        Self::from_url_with_octree_concurrency(url, None)
    }

    pub fn from_url_with_octree_concurrency(
        url: &str,
        max_concurrent_requests: Option<usize>,
    ) -> Result<PotreeAssetSource<HttpSource>, ByteSourceError> {
        let suffix_start = url.find(['?', '#']).unwrap_or(url.len());
        let (url_path, suffix) = url.split_at(suffix_start);
        let base_url = if url_path.ends_with('/') {
            // remove leading /
            url_path.trim_end_matches('/').to_string()
        } else {
            match url_path.rfind('/') {
                // remove last part of the url if it ends with (metadata.json, hierarchy.bin or
                // octree.bin)
                Some(index) => {
                    let (path, end) = url_path.split_at(index);
                    match &end[1..] {
                        "metadata.json" | "hierarchy.bin" | "octree.bin" => path.to_string(),
                        _ => url_path.to_string(),
                    }
                }
                None => url_path.to_string(),
            }
        };

        Ok(PotreeAssetSource {
            metadata: HttpSource::open(&potree_file_url(&base_url, "metadata.json", suffix))?,
            hierarchy: HttpSource::open(&potree_file_url(&base_url, "hierarchy.bin", suffix))?,
            node_fetch: NodeFetch::Http {
                base_url: format!("{base_url}/octree"),
                suffix: suffix.to_string(),
                octree: HttpSource::open_with_concurrency(
                    &potree_file_url(&base_url, "octree.bin", suffix),
                    max_concurrent_requests,
                )?,
            },
            layout: OnceLock::new(),
        })
    }
}

// Cache the single-file handle only after the metadata selects that layout.
fn file_octree<'a>(
    base_path: &std::path::Path,
    octree: &'a OnceLock<FileSource>,
) -> Result<&'a FileSource, ByteSourceError> {
    if octree.get().is_none() {
        let source = FileSource::open(base_path.with_file_name("octree.bin"))?;
        let _ = octree.set(source);
    }
    Ok(octree.get().expect("single-file source was initialized"))
}

fn potree_file_url(base_url: &str, filename: &str, suffix: &str) -> String {
    format!("{base_url}/{filename}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_query_is_appended_after_the_potree_filename() {
        assert_eq!(
            potree_file_url(
                "https://example.com/site/pcd",
                "metadata.json",
                "?v=revision"
            ),
            "https://example.com/site/pcd/metadata.json?v=revision"
        );
    }

    #[test]
    fn source_file_urls_are_normalized_to_the_potree_directory() {
        let source = PotreeAssetSource::<HttpSource>::from_url(
            "https://example.com/site/pcd/metadata.json?v=revision",
        )
        .expect("valid HTTP source");

        assert_eq!(
            source.metadata.url,
            "https://example.com/site/pcd/metadata.json?v=revision"
        );
        assert_eq!(
            source.hierarchy.url,
            "https://example.com/site/pcd/hierarchy.bin?v=revision"
        );
    }
}
