//! Opt-in page residency; whole-file decoding is only bounded staging.
use super::super::{
    ChunkLoad, SogLodSettings, SogStream, SourceStorage, SplatRange, diagnostics, distance,
    packed_reservation, scheduling, selection,
};
use super::{Key, PageCache};
use crate::renderer::source::SourceData;
use std::collections::BTreeSet;

impl SogStream {
    /// Progress already-admitted copies while GPU preparation/readiness waits.
    /// Does not select ranges, request files, evict pins or acknowledge uploads.
    pub(in crate::streaming::runtime) fn progress_pages(&mut self, capacity: usize) {
        let Some(mut pages) = self.pages.take() else {
            return;
        };
        let _timer = self
            .cpu_timings
            .start(crate::diagnostics::sog_cpu::CpuStage::RelocationPlan);
        let result = self.copy_pending_page(&mut pages, capacity as u64);
        self.pages = Some(pages);
        match result {
            Ok(true) => {
                self.next_update = 0.;
                self.pending_work = true;
            }
            Ok(false) => {}
            Err(error) => {
                self.preparing_pages = false;
                self.last_error = Some(error);
            }
        }
    }

    fn copy_pending_page(&mut self, pages: &mut PageCache, capacity: u64) -> Result<bool, String> {
        if pages.pending.is_empty() {
            return Ok(false);
        }
        for (file, entry) in self.entries.iter_mut().enumerate() {
            if let Some(chunk) = &mut entry.ready {
                let SourceData::Packed(source) = &mut chunk.cloud else {
                    return Err("page staging received expanded data".into());
                };
                pages.observe(file, source)?;
                pages.reuse_palette(file, std::sync::Arc::make_mut(source))?;
            }
        }
        // The environment has an upper-bound count until its metadata arrives.
        pages.pending = pages
            .pending
            .iter()
            .copied()
            .filter(|key| pages.page_count(*key).is_some())
            .collect();
        let next = pages
            .pending
            .iter()
            .find(|key| !pages.pages.contains_key(key) && self.entries[key.file].ready.is_some())
            .copied();
        let needed = pages.pending.iter().copied().collect();
        let mut prospective: BTreeSet<_> = pages.pages.keys().copied().collect();
        prospective.extend(next);
        let staging =
            self.trim_completed_sources(pages, &needed, pages.cost(&prospective), 0, capacity);
        if let Some(key) = next {
            let source = self.entries[key.file]
                .ready
                .as_ref()
                .unwrap()
                .cloud
                .packed()?;
            pages.insert(
                key,
                source,
                capacity.saturating_sub(staging),
                self.generation,
            )?;
            self.entries[key.file].last_used = self.generation;
        }
        self.preparing_pages = pages
            .pending
            .iter()
            .any(|key| !pages.pages.contains_key(key) && self.entries[key.file].ready.is_some());
        Ok(next.is_some())
    }

    /// Enable packed page residency before streaming starts.
    pub(in crate::streaming::runtime) fn enable_pages(&mut self) -> Result<(), String> {
        if self.storage != SourceStorage::Packed || self.generation != 0 {
            return Err("page residency requires packed storage before streaming starts".into());
        }
        let mut counts = self.manifest.file_counts.clone();
        if self.environment_index().is_some() {
            counts.push(100_000);
        }
        self.pages = Some(PageCache::new(counts)?);
        Ok(())
    }

    /// Advance bounded page admission and source requests for the current view.
    pub(in crate::streaming::runtime) fn tick_pages(
        &mut self,
        view: selection::View,
        now: f64,
        settings: &SogLodSettings,
        gpu_limit: usize,
        gpu_source_bytes: Option<u64>,
        start: impl FnMut(String, Option<usize>, usize, f64) -> ChunkLoad,
    ) -> bool {
        let _timer = self
            .cpu_timings
            .start(crate::diagnostics::sog_cpu::CpuStage::RelocationPlan);
        let mut pages = self.pages.take().expect("paged stream");
        self.preparing_pages = false;
        self.admission = Default::default();
        let result = self.tick_page_cache(
            &mut pages,
            view,
            now,
            settings,
            gpu_limit,
            gpu_source_bytes.unwrap_or(u64::MAX),
            start,
        );
        self.pages = Some(pages);
        match result {
            Ok(changed) => changed,
            Err(error) => {
                self.last_error = Some(error);
                false
            }
        }
    }

    fn page_staging_bytes(&self) -> u64 {
        self.entries.iter().fold(0u64, |sum, entry| {
            sum.saturating_add(if let Some(chunk) = &entry.ready {
                chunk.cloud.admission_bytes()
            } else if entry.loading.is_some() {
                packed_reservation(entry.reserved_splats.unwrap_or(0)) as u64
            } else {
                0
            })
        })
    }

    /// Full decoded files are an opportunistic reuse cache, never permanent
    /// pins. Reclaim least-recently-copied completed files before pages or new
    /// decoder reservations are refused. In-flight and uncopied sources stay live.
    fn trim_completed_sources(
        &mut self,
        pages: &PageCache,
        needed: &BTreeSet<Key>,
        page_bytes: u64,
        extra: u64,
        capacity: u64,
    ) -> u64 {
        let mut staging = self.page_staging_bytes();
        let mut candidates: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(file, entry)| {
                entry.ready.is_some()
                    && !needed
                        .iter()
                        .any(|key| key.file == *file && !pages.pages.contains_key(key))
            })
            .map(|(file, entry)| (entry.last_used, file))
            .collect();
        candidates.sort_unstable();
        for (_, file) in candidates {
            if page_bytes.saturating_add(staging).saturating_add(extra) <= capacity {
                break;
            }
            let chunk = self.entries[file].ready.take().unwrap();
            staging = staging.saturating_sub(chunk.cloud.admission_bytes());
            self.entries[file].attempts = 0;
        }
        staging
    }

    /// Reserve one complete decoder result alongside all destination pages.
    /// Additional concurrent requests are admitted against that same union.
    fn page_decode_reserve(&self, pages: &PageCache, keys: &BTreeSet<Key>) -> u64 {
        keys.iter()
            .filter(|key| !pages.pages.contains_key(key))
            .map(|key| {
                self.entries[key.file].ready.as_ref().map_or_else(
                    || packed_reservation(pages.layouts[key.file].count) as u64,
                    |chunk| chunk.cloud.admission_bytes(),
                )
            })
            .max()
            .unwrap_or(0)
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_page_cache(
        &mut self,
        pages: &mut PageCache,
        view: selection::View,
        now: f64,
        settings: &SogLodSettings,
        gpu_limit: usize,
        gpu_capacity: u64,
        mut start: impl FnMut(String, Option<usize>, usize, f64) -> ChunkLoad,
    ) -> Result<bool, String> {
        let capacity = settings.cache_bytes as u64;
        for (file, entry) in self.entries.iter_mut().enumerate() {
            if let Some(chunk) = &mut entry.ready {
                let SourceData::Packed(source) = &mut chunk.cloud else {
                    return Err("page staging received expanded data".into());
                };
                pages.observe(file, source)?;
                pages.reuse_palette(file, std::sync::Arc::make_mut(source))?;
            }
        }
        let env = self.environment_index().map(|file| SplatRange {
            file,
            offset: 0,
            count: pages.layouts[file].count,
        });
        let coarse: Vec<_> = self
            .manifest
            .leaves
            .iter()
            .map(|leaf| leaf.lods.last().unwrap().1)
            .chain(env)
            .collect();
        let baseline = pages.union(coarse.iter().copied())?;
        let floor = self
            .manifest
            .leaves
            .iter()
            .map(|leaf| leaf.lods.last().unwrap().1.count)
            .fold(0usize, usize::saturating_add)
            .saturating_add(env.map_or(0, |range| range.count));
        if floor > gpu_limit {
            return Err("Coarsest SOG coverage exceeds the GPU storage binding limit".into());
        }
        self.effective = self.effective.max(floor);
        let targets = selection::target_ranges(
            &self.manifest,
            view,
            self.effective.saturating_sub(env.map_or(0, |r| r.count)),
        );
        let mut desired: Vec<_> = self
            .manifest
            .leaves
            .iter()
            .zip(&targets)
            .map(|(leaf, target)| target.map(|_| leaf.lods.last().unwrap().1))
            .collect();
        let fits = |keys: &BTreeSet<Key>| {
            let cost = pages.cost(keys);
            cost <= gpu_capacity
                && cost.saturating_add(self.page_decode_reserve(pages, keys)) <= capacity
        };
        if !fits(&baseline) {
            return Err(
                "Coarsest SOG pages plus decode staging exceed source admission budget".into(),
            );
        }
        // The selected-count policy remains authoritative. Source admission may
        // only choose a coarser representation, never spend extra selected rows.
        let ideal_keys = pages.union(
            coarse
                .iter()
                .copied()
                .chain(targets.iter().flatten().copied()),
        )?;
        if fits(&ideal_keys)
            && targets
                .iter()
                .flatten()
                .all(|range| pages.covers(*range) || self.entries[range.file].can_retry())
        {
            // Most camera evaluations fit the complete destination. Its union
            // dominates every intermediate subset, so no per-leaf search is needed.
            desired = targets.clone();
        } else {
            let mut order: Vec<_> = (0..targets.len()).collect();
            order.sort_by(|&a, &b| {
                distance::full_bounds_squared(self.manifest.leaves[a].bound, view.eye, view.forward)
                    .total_cmp(&distance::full_bounds_squared(
                        self.manifest.leaves[b].bound,
                        view.eye,
                        view.forward,
                    ))
            });
            for id in order {
                let Some(target) = targets[id] else {
                    continue;
                };
                let leaf = &self.manifest.leaves[id];
                let first = leaf
                    .lods
                    .iter()
                    .position(|(_, range)| *range == target)
                    .unwrap();
                for &(_, candidate) in &leaf.lods[first..] {
                    if !pages.covers(candidate) && !self.entries[candidate.file].can_retry() {
                        continue;
                    }
                    let previous = desired[id].replace(candidate);
                    let keys = pages.union(
                        coarse
                            .iter()
                            .copied()
                            .chain(desired.iter().flatten().copied()),
                    )?;
                    if fits(&keys) {
                        break;
                    }
                    desired[id] = previous;
                }
            }
        }
        self.overlay_intent.targets = targets.clone();
        self.overlay_intent.capped_leaves =
            targets.iter().zip(&desired).map(|(a, b)| a != b).collect();
        self.admission.source_limited = self.overlay_intent.capped_leaves.iter().any(|&v| v);
        let wanted = pages.union(
            coarse
                .iter()
                .copied()
                .chain(desired.iter().flatten().copied()),
        )?;
        let mut pinned = wanted.clone();
        pinned.extend(
            pages.union(
                self.active
                    .iter()
                    .chain(&self.submitted)
                    .chain(&self.displayed)
                    .copied(),
            )?,
        );
        // Reuse recent decoded files when there is spare room. They are the
        // first eviction candidates once destination pages need that capacity.
        let staging = self.trim_completed_sources(pages, &wanted, pages.cost(&pinned), 0, capacity);
        let transition_fits = pages.cost(&pinned) <= gpu_capacity
            && pages
                .cost(&pinned)
                .saturating_add(staging.max(self.page_decode_reserve(pages, &wanted)))
                <= capacity;
        if !transition_fits {
            pages.pending.clear();
            // Publish a covered bridge whose pages belong to the destination.
            // Acquisition waits until displayed/submitted owners retire.
            let mut bridge = Vec::new();
            for (leaf, target) in self.manifest.leaves.iter().zip(&desired) {
                let Some(target) = target else {
                    continue;
                };
                let first = leaf.lods.iter().position(|(_, r)| r == target).unwrap();
                let range = leaf.lods[first..].iter().find_map(|(_, r)| {
                    (pages.covers(*r)
                        && pages
                            .keys(*r)
                            .is_ok_and(|keys| keys.iter().all(|k| wanted.contains(k))))
                    .then_some(*r)
                });
                // During startup keep existing coverage until the baseline is ready.
                let Some(range) = range else {
                    self.pending_work = true;
                    return Ok(false);
                };
                if range.count > 0 {
                    bridge.push(range);
                }
            }
            if let Some(env) = env.filter(|r| pages.covers(*r)) {
                bridge.push(env);
            }
            self.pending_work = true;
            self.overlay_intent.transition_blocked = true;
            let changed = bridge != self.active;
            self.active = bridge;
            return Ok(changed);
        }
        pages.touch(&pinned, self.generation);
        if !pages.make_room(&pinned, staging, capacity) {
            return Err("SOG pages are waiting for pinned source headroom".into());
        }
        let queue = scheduling::requests(
            &self.manifest,
            &desired,
            &self.displayed,
            view,
            self.environment_index(),
        );
        pages.pending = queue
            .iter()
            .flat_map(|file| wanted.iter().filter(move |key| key.file == *file).copied())
            .collect();
        // At most one 65K page copy per update. The same bounded operation also
        // runs on updates spent waiting for renderer preparation/readiness.
        self.copy_pending_page(pages, capacity)?;
        let mut inflight = self.entries.iter().filter(|e| e.loading.is_some()).count();
        for &file in &queue {
            if inflight >= settings.concurrent_chunks.clamp(1, 4) {
                break;
            }
            if !wanted
                .iter()
                .any(|key| key.file == file && !pages.pages.contains_key(key))
                || !self.entries[file].eligible(now)
            {
                continue;
            }
            let reserve = packed_reservation(pages.layouts[file].count) as u64;
            // Reserve destination page growth as well as all in-flight decoders.
            // Completed decoded caches must yield before acquisition is blocked.
            let staging =
                self.trim_completed_sources(pages, &wanted, pages.cost(&pinned), reserve, capacity);
            if pages
                .cost(&pinned)
                .saturating_add(staging)
                .saturating_add(reserve)
                > capacity
            {
                continue;
            }
            if !pages.make_room(&pinned, staging.saturating_add(reserve), capacity) {
                continue;
            }
            let (path, expected, max) = if file < self.manifest.filenames.len() {
                (
                    &self.manifest.filenames[file],
                    Some(self.manifest.file_counts[file]),
                    self.manifest.file_counts[file],
                )
            } else {
                (self.manifest.environment.as_ref().unwrap(), None, 100_000)
            };
            self.entries[file].attempts += 1;
            self.entries[file].reserved_splats = Some(expected.unwrap_or(max));
            self.entries[file].loading = Some(start(
                format!("{}{path}", self.base_url),
                expected,
                max,
                now,
            ));
            inflight += 1;
        }
        let selected_leaves: Vec<_> = self
            .manifest
            .leaves
            .iter()
            .zip(&desired)
            .map(|(leaf, target)| {
                target.and_then(|target| {
                    let first = leaf.lods.iter().position(|(_, r)| *r == target).unwrap();
                    leaf.lods[first..]
                        .iter()
                        .find_map(|(_, r)| pages.covers(*r).then_some(*r))
                })
            })
            .collect();
        let mut selected: Vec<_> = selected_leaves
            .iter()
            .flatten()
            .copied()
            .filter(|r| r.count > 0)
            .collect();
        if let Some(env) =
            env.filter(|r| pages.layouts[r.file].known && pages.covers(*r) && r.count > 0)
        {
            selected.push(env);
        }
        self.preparing_pages = wanted
            .iter()
            .any(|key| !pages.pages.contains_key(key) && self.entries[key.file].ready.is_some());
        self.pending_work = self.preparing_pages
            || self.entries.iter().any(|e| e.loading.is_some())
            || wanted
                .iter()
                .any(|key| !pages.pages.contains_key(key) && self.entries[key.file].can_retry());
        self.overlay_intent.queued_targets = desired
            .iter()
            .map(|target| {
                target.filter(|r| {
                    !pages.covers(*r)
                        && (self.entries[r.file].loading.is_some()
                            || self.entries[r.file].ready.is_some())
                })
            })
            .collect();
        if let Some(trace) = &mut self.diagnostics {
            for (id, leaf) in self
                .manifest
                .leaves
                .iter()
                .enumerate()
                .take(diagnostics::MAX_LEAF_DIAGNOSTICS)
            {
                let lod = |range: Option<SplatRange>| {
                    range
                        .and_then(|r| leaf.lods.iter().find(|(_, candidate)| *candidate == r))
                        .map(|(lod, _)| *lod as u32)
                };
                trace.push(diagnostics::SogLeafDiagnostic {
                    leaf: id,
                    projected_bounds: diagnostics::project(leaf.bound, view.clip_from_local),
                    culled: !selection::intersects(leaf.bound, view.clip_from_local),
                    desired_lod: lod(targets[id]),
                    chosen_lod: lod(selected_leaves[id]),
                    range: selected_leaves[id],
                    reason: if !selection::intersects(leaf.bound, view.clip_from_local) {
                        "offscreen coarse coverage"
                    } else if targets[id] != desired[id] {
                        "source transition"
                    } else if selected_leaves[id] != desired[id] {
                        "loading"
                    } else {
                        "selected target"
                    },
                });
            }
        }
        if !wanted
            .iter()
            .any(|key| !pages.pages.contains_key(key) && !self.entries[key.file].can_retry())
        {
            self.last_error = None;
        }
        let changed = selected != self.active;
        self.active = selected;
        Ok(changed)
    }
}
