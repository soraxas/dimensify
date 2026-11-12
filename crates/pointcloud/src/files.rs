//! Selected local files retain their directory structure and are read on demand.
use bevy_pointcloud::{ByteSource, ByteSourceError};
use std::{collections::BTreeMap, sync::Arc};

/// A local random-access file. Browser files retain a Blob handle, not a copy
/// of the complete dataset. Native hosts can also provide in-memory bytes.
#[derive(Clone)]
pub struct PointcloudFile(Arc<FileData>);
enum FileData {
    Bytes(Arc<[u8]>),
    #[cfg(target_arch = "wasm32")]
    Browser(send_wrapper::SendWrapper<web_sys::File>),
}
impl std::fmt::Debug for PointcloudFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointcloudFile").finish_non_exhaustive()
    }
}
impl PointcloudFile {
    /// Retain bytes already owned by an application.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self(Arc::new(FileData::Bytes(bytes.into())))
    }
    /// Retain a browser file for lazy Blob.slice reads on the browser thread.
    #[cfg(target_arch = "wasm32")]
    pub fn from_browser(file: web_sys::File) -> Self {
        Self(Arc::new(FileData::Browser(send_wrapper::SendWrapper::new(
            file,
        ))))
    }
    fn len(&self) -> u64 {
        match self.0.as_ref() {
            FileData::Bytes(bytes) => bytes.len() as u64,
            #[cfg(target_arch = "wasm32")]
            FileData::Browser(file) => file.size() as u64,
        }
    }
}
fn file_error(message: impl Into<String>) -> ByteSourceError {
    ByteSourceError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    ))
}
impl ByteSource for PointcloudFile {
    async fn read_to_end(&self, offset: u64) -> Result<Vec<u8>, ByteSourceError> {
        self.read_range(
            offset,
            self.len()
                .checked_sub(offset)
                .ok_or_else(|| file_error("File offset exceeds its length"))?,
        )
        .await
    }
    async fn read_range(&self, offset: u64, length: u64) -> Result<Vec<u8>, ByteSourceError> {
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= self.len())
            .ok_or_else(|| file_error("File range exceeds its length"))?;
        match self.0.as_ref() {
            FileData::Bytes(bytes) => Ok(bytes[offset as usize..end as usize].to_vec()),
            #[cfg(target_arch = "wasm32")]
            FileData::Browser(file) => {
                let slice = file
                    .slice_with_f64_and_f64(offset as f64, end as f64)
                    .map_err(|e| file_error(format!("Cannot slice file: {e:?}")))?;
                let buffer = wasm_bindgen_futures::JsFuture::from(slice.array_buffer())
                    .await
                    .map_err(|e| file_error(format!("Cannot read file: {e:?}")))?;
                Ok(js_sys::Uint8Array::new(&buffer).to_vec())
            }
        }
    }
    async fn size(&self) -> Result<Option<u64>, ByteSourceError> {
        Ok(Some(self.len()))
    }
}

/// A selected Potree 2 directory, retaining relative paths for node streaming.
#[derive(Clone, Debug)]
pub struct PotreeFiles {
    files: Arc<BTreeMap<String, PointcloudFile>>,
    name: String,
}
impl PotreeFiles {
    /// Accept one directory tree, with exactly one metadata.json and its siblings.
    pub fn new(files: impl IntoIterator<Item = (String, PointcloudFile)>) -> Result<Self, String> {
        let mut selected = BTreeMap::new();
        for (path, file) in files {
            let path = path.replace('\\', "/");
            if path.starts_with('/')
                || path
                    .split('/')
                    .any(|p| p == ".." || p == "." || p.is_empty())
            {
                return Err("Invalid relative path in selected folder".into());
            }
            if selected.insert(path.clone(), file).is_some() {
                return Err(format!("Duplicate selected file: {path}"));
            }
        }
        let metadata: Vec<_> = selected
            .keys()
            .filter(|path| path.rsplit('/').next() == Some("metadata.json"))
            .cloned()
            .collect();
        if metadata.len() != 1 {
            return Err(
                "Choose one Potree folder containing metadata.json, hierarchy.bin and octree data"
                    .into(),
            );
        }
        let prefix = metadata[0].strip_suffix("metadata.json").unwrap();
        let files: BTreeMap<_, _> = selected
            .iter()
            .filter_map(|(path, file)| {
                path.strip_prefix(prefix)
                    .map(|path| (path.to_owned(), file.clone()))
            })
            .collect();
        if !files.contains_key("hierarchy.bin")
            || !(files.contains_key("octree.bin")
                || files
                    .keys()
                    .any(|p| p.starts_with("octree/") && p.ends_with(".bin")))
        {
            return Err("Incomplete Potree dataset. Choose or drop the whole folder, including hierarchy.bin and octree data".into());
        }
        let name = prefix
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("Local Potree dataset")
            .to_owned();
        Ok(Self {
            files: Arc::new(files),
            name,
        })
    }
    pub(super) fn source(
        &self,
    ) -> Result<bevy_pointcloud::potree::PotreeAssetSource<PointcloudFile>, String> {
        let files = self.files.clone();
        bevy_pointcloud::potree::PotreeAssetSource::from_directory(move |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| file_error(format!("Missing Potree file: {path}")))
        })
        .map_err(|e| e.to_string())
    }
    /// Display name of the selected dataset directory.
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_selection_retains_relative_nodes_and_accepts_both_layouts() {
        for data in ["tree/octree.bin", "tree/octree/r.bin"] {
            let files = PotreeFiles::new(
                ["tree/metadata.json", "tree/hierarchy.bin", data]
                    .map(|name| (name.into(), PointcloudFile::from_bytes(vec![1, 2, 3]))),
            )
            .unwrap();
            assert_eq!(files.name(), "tree");
            let key = if data.ends_with("/r.bin") {
                "octree/r.bin"
            } else {
                "octree.bin"
            };
            assert_eq!(
                futures::executor::block_on(files.files[key].read_range(1, 2)).unwrap(),
                [2, 3]
            );
        }
    }
    #[test]
    fn incomplete_or_ambiguous_directories_are_rejected() {
        for names in [
            vec!["metadata.json"],
            vec!["a/metadata.json", "b/metadata.json"],
            vec!["../metadata.json", "hierarchy.bin", "octree.bin"],
        ] {
            assert!(
                PotreeFiles::new(
                    names
                        .into_iter()
                        .map(|name| (name.into(), PointcloudFile::from_bytes(vec![])))
                )
                .is_err()
            );
        }
    }
    #[test]
    fn file_ranges_are_checked_without_panicking() {
        futures::executor::block_on(async {
            let file = PointcloudFile::from_bytes(vec![1, 2, 3]);
            assert_eq!(file.read_to_end(1).await.unwrap(), [2, 3]);
            assert!(file.read_to_end(4).await.is_err());
            assert!(file.read_range(u64::MAX, 2).await.is_err());
        });
    }
}
