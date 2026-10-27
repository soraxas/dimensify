//! Range-page CPU ownership and admission for the experimental packed loader.
use super::manifest::SplatRange;
use crate::renderer::{DecodedChunk, source::SourceData};
use bevy_gaussian_splatting::io::sog_packed::{DecodeLimits, PackedSog};
use std::collections::{BTreeMap, BTreeSet};
const PAGE: usize = 65_536;
mod stream;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    file: usize,
    page: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) count: usize,
    planes: u64,
    palette: u64,
    pub(super) known: bool,
}
struct Page {
    chunk: DecodedChunk,
    used: u64,
}

pub(super) struct PageCache {
    pub(super) layouts: Vec<Layout>,
    pages: BTreeMap<Key, Page>,
    /// Ordered destination pages whose growth was already reserved by policy.
    pending: Vec<Key>,
}
impl PageCache {
    fn new(counts: Vec<usize>) -> Result<Self, String> {
        if counts.is_empty()
            || counts.iter().any(|&n| n > u32::MAX as usize)
            || counts
                .len()
                .checked_mul((u32::MAX as usize / PAGE) + 1)
                .is_none()
        {
            return Err("invalid paged source counts".into());
        }
        Ok(Self {
            layouts: counts
                .into_iter()
                .map(|count| Layout {
                    count,
                    planes: 6,
                    palette: 4 * 1024 * 1024,
                    known: false,
                })
                .collect(),
            pages: BTreeMap::new(),
            pending: Vec::new(),
        })
    }
    fn observe(&mut self, file: usize, source: &PackedSog) -> Result<(), String> {
        let old = self
            .layouts
            .get_mut(file)
            .ok_or("page source out of bounds")?;
        let next = Layout {
            count: source.count() as usize,
            planes: if source.higher_order().is_some() {
                6
            } else {
                5
            },
            palette: source.palette_texture_bytes(),
            known: true,
        };
        if old.known && *old != next {
            return Err("immutable page source layout changed".into());
        }
        *old = next;
        Ok(())
    }
    fn reuse_palette(&self, file: usize, source: &mut PackedSog) -> Result<(), String> {
        if let Some((_, page)) = self.pages.iter().find(|(key, _)| key.file == file) {
            source
                .reuse_palette(page.chunk.cloud.packed()?)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn keys(&self, range: SplatRange) -> Result<Vec<Key>, String> {
        let layout = self
            .layouts
            .get(range.file)
            .ok_or("page source out of bounds")?;
        let end = range
            .offset
            .checked_add(range.count)
            .filter(|&end| end <= layout.count)
            .ok_or("page range exceeds source")?;
        if range.count == 0 {
            return Ok(Vec::new());
        }
        Ok((range.offset / PAGE..=(end - 1) / PAGE)
            .map(|page| Key {
                file: range.file,
                page,
            })
            .collect())
    }
    fn union(&self, ranges: impl IntoIterator<Item = SplatRange>) -> Result<BTreeSet<Key>, String> {
        let mut keys = BTreeSet::new();
        for range in ranges {
            keys.extend(self.keys(range)?);
        }
        Ok(keys)
    }
    fn page_count(&self, key: Key) -> Option<usize> {
        let source = self.layouts.get(key.file)?;
        let start = key.page.checked_mul(PAGE)?;
        (start < source.count).then(|| (source.count - start).min(PAGE))
    }
    // Conservative common CPU/GPU payload envelope: per-page decoded metadata
    // plus GPU LUT/fallback overhead. It dominates either separate payload.
    fn cost(&self, keys: &BTreeSet<Key>) -> u64 {
        let mut bytes = 0u64;
        let mut files = BTreeSet::new();
        for &key in keys {
            let Some(count) = self.page_count(key) else {
                return u64::MAX;
            };
            let width = count.min(256);
            let padded = width * count.div_ceil(width);
            let layout = self.layouts[key.file];
            bytes = bytes
                .saturating_add(padded as u64 * 4 * layout.planes)
                .saturating_add(std::mem::size_of::<DecodedChunk>().max(48) as u64)
                .saturating_add(if layout.planes == 5 { 4 } else { 0 });
            if files.insert(key.file) {
                bytes = bytes
                    .saturating_add(layout.palette + 4096 + if layout.planes == 5 { 4 } else { 0 });
            }
        }
        bytes
    }
    /// Return the page-cache resident allocation size.
    pub(super) fn resident_bytes(&self) -> u64 {
        self.cost(&self.pages.keys().copied().collect())
    }
    /// Report whether the resident pages cover the selected range.
    pub(super) fn covers(&self, range: SplatRange) -> bool {
        self.keys(range)
            .is_ok_and(|keys| keys.iter().all(|key| self.pages.contains_key(key)))
    }
    /// Borrow one immutable resident page source.
    pub(super) fn source(&self, id: usize) -> Option<&DecodedChunk> {
        let file = id % self.layouts.len();
        let page = id / self.layouts.len();
        self.pages.get(&Key { file, page }).map(|page| &page.chunk)
    }
    /// Split selected ranges at immutable page boundaries.
    pub(super) fn split(&self, ranges: &[SplatRange]) -> Result<Vec<SplatRange>, String> {
        let mut out = Vec::new();
        for &range in ranges {
            if !self.covers(range) {
                return Err("page range is not resident".into());
            }
            let end = range.offset + range.count;
            for key in self.keys(range)? {
                let start = (key.page * PAGE).max(range.offset);
                let stop = ((key.page + 1) * PAGE).min(end);
                out.push(SplatRange {
                    file: key.page * self.layouts.len() + key.file,
                    offset: start - key.page * PAGE,
                    count: stop - start,
                });
            }
        }
        Ok(out)
    }
    fn insert(
        &mut self,
        key: Key,
        source: &PackedSog,
        capacity: u64,
        used: u64,
    ) -> Result<(), String> {
        if let Some(page) = self.pages.get_mut(&key) {
            page.used = used;
            return Ok(());
        }
        let count = self.page_count(key).ok_or("page out of bounds")?;
        if source.count() as usize != self.layouts[key.file].count || !self.layouts[key.file].known
        {
            return Err("page source layout not observed".into());
        }
        let mut prospective: BTreeSet<_> = self.pages.keys().copied().collect();
        prospective.insert(key);
        let required = self.cost(&prospective);
        if required > capacity {
            return Err("page cache capacity exceeded".into());
        }
        let start = key.page * PAGE;
        let source = source
            .copy_range(
                start as u32..(start + count) as u32,
                DecodeLimits {
                    max_texture_dimension: 8192,
                    max_texture_bytes: required,
                },
            )
            .map_err(|e| e.to_string())?;
        self.pages.insert(
            key,
            Page {
                chunk: DecodedChunk {
                    count,
                    cloud: SourceData::PackedPage {
                        original_file: key.file,
                        source,
                    },
                },
                used,
            },
        );
        Ok(())
    }
    fn touch(&mut self, keys: &BTreeSet<Key>, used: u64) {
        for key in keys {
            if let Some(page) = self.pages.get_mut(key) {
                page.used = used;
            }
        }
    }
    fn make_room(&mut self, pinned: &BTreeSet<Key>, extra: u64, capacity: u64) -> bool {
        let mut keep: BTreeSet<_> = self.pages.keys().copied().collect();
        // Pins include destination pages not copied yet: reserve their growth
        // now, otherwise obsolete cached pages can exhaust the copy allowance.
        if self.cost(pinned).saturating_add(extra) > capacity {
            return false;
        }
        keep.extend(pinned.iter().copied());
        let mut candidates: Vec<_> = self
            .pages
            .iter()
            .filter(|(key, _)| !pinned.contains(key))
            .map(|(key, page)| (*key, page.used))
            .collect();
        candidates.sort_by_key(|(key, used)| (*used, *key));
        for (key, _) in candidates {
            if self.cost(&keep).saturating_add(extra) <= capacity {
                break;
            }
            keep.remove(&key);
        }
        self.pages.retain(|key, _| keep.contains(key));
        true
    }
}
