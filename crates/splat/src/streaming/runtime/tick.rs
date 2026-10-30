//! Due policy evaluation: selection, relocation planning and request dispatch.
use super::*;

impl SogStream {
    pub(super) fn tick(
        &mut self,
        view: selection::View,
        now: f64,
        settings: &SogLodSettings,
        gpu_limit: usize,
        gpu_source_bytes: Option<u64>,
        mut start: impl FnMut(String, Option<usize>, usize, f64) -> ChunkLoad,
    ) -> bool {
        use crate::streaming::SogLodPolicy;
        // Match the reference lifecycle: resolve accepted A→B completions
        // before policy can supersede B with C. This promotes only admitted
        // ready ranges, never dispatches the old request queue, and keeps
        // displayed/submitted ownership until renderer acknowledgement.
        self.poll_loads(now);
        if self.pages.is_none()
            && !self.snapshot.is_pending()
            && !self.preparing_upload
            && self.submission_error.is_none()
            && self.replacements.dirty
        {
            self.progress_replacements(settings, gpu_limit, gpu_source_bytes);
        }
        let config = (settings.lod_policy, settings.target_pixels);
        if self.quality_config != Some(config) {
            self.quality_history.clear();
            self.relocation_files = None;
            self.quality_config = Some(config);
        }
        if settings.lod_policy == SogLodPolicy::ScreenSpace {
            if let Some(pixels) = view.pixels {
                screen_space::targets(
                    &self.manifest,
                    pixels,
                    settings.target_pixels as f32,
                    &mut self.quality_history,
                );
            } else {
                self.quality_history = vec![0; self.manifest.leaves.len()];
            }
        }
        let quality = self.quality_history.clone();
        let mut view = selection::View {
            quality: (settings.lod_policy == SogLodPolicy::ScreenSpace)
                .then_some(quality.as_slice()),
            ..view
        };
        self.generation += 1;
        self.requested = settings.splat_budget;
        self.effective = settings.splat_budget.min(gpu_limit);
        self.pending_work = self.entries.iter().any(|entry| entry.loading.is_some());
        self.overlay_intent = default();
        // A due policy owns supersession. Never dispatch an old queue if this
        // evaluation rejects or replaces its destinations.
        self.replacements.requests.clear();
        self.replacements.leaves.clear();
        self.replacements.dirty = false;
        if let Some(trace) = self.diagnostics.as_mut() {
            trace.clear();
        }
        let diagnostic_projection = view.clip_from_local;
        view.clip_from_local = match settings.effective_visibility() {
            SogVisibilityPolicy::All => None,
            SogVisibilityPolicy::Frustum => view
                .clip_from_local
                .or_else(|| view.pixels.map(|pixels| pixels.clip_from_local)),
        };
        if self.pages.is_some() {
            return self.tick_pages(view, now, settings, gpu_limit, gpu_source_bytes, start);
        }
        // Keep CPU decoder padding separate from logical GPU source planes.
        let packed = self.storage == SourceStorage::Packed;
        let capacity = if packed {
            settings.cache_bytes
        } else {
            settings.cache_bytes / chunk::DecodedChunk::bytes_per_splat()
        };
        // Environment is independent of the tree; reserve bounded space before its metadata arrives.
        let env_cap = if self.environment_index().is_some() {
            capacity.min(100_000)
        } else {
            0
        };
        let coarse = self.manifest.coarse_files();
        let logical_costs: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                entry.ready.as_ref().map_or_else(
                    || {
                        entry.reserved_splats.unwrap_or_else(|| {
                            self.manifest.file_counts.get(i).copied().unwrap_or(env_cap)
                        })
                    },
                    |chunk| chunk.count,
                )
            })
            .collect();
        // The SOG decoder appends 32 - (count % 32), including a full tile for
        // aligned counts. Keep its reservation until the decode has completed.
        // A warm-only entry still uses the cold reservation envelope here:
        // an earlier request may trim its expendable copy during this tick.
        // Freezing its smaller warm size would underreserve a later download.
        let costs: Vec<_> = logical_costs
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if packed {
                    return self.entries[i].ready.as_ref().map_or_else(
                        || packed_reservation(*n),
                        |chunk| chunk.cloud.retained_bytes() as usize,
                    );
                }
                self.entries[i]
                    .ready
                    .as_ref()
                    .map_or_else(|| cache::reserved_rows(*n), |chunk| chunk.padded_rows())
            })
            .collect();
        let mut pinned = vec![false; self.entries.len()];
        for i in coarse.iter().copied().chain(self.environment_index()) {
            pinned[i] = true;
        }
        let baseline = pinned
            .iter()
            .zip(&costs)
            .filter(|(pin, _)| **pin)
            .fold(0usize, |sum, (_, cost)| sum.saturating_add(*cost));
        let gpu_capacity = gpu_source_bytes.map(|bytes| {
            usize::try_from(if packed {
                bytes
            } else {
                bytes / renderer::source_bytes_per_splat()
            })
            .unwrap_or(usize::MAX)
        });
        // Uploaded whole files retain only a small CPU descriptor. GPU source
        // admission still charges their full allocation, independently of CPU
        // staging, so dropping texels never invents GPU memory headroom.
        let packed_gpu_costs: Vec<_> = if packed {
            logical_costs
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    self.entries[i].ready.as_ref().map_or_else(
                        || packed_reservation(*n),
                        |chunk| chunk.cloud.admission_bytes() as usize,
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        let gpu_costs = if packed {
            &packed_gpu_costs
        } else {
            &logical_costs
        };
        // These are upper bounds only: whole-file/padded old-new unions below
        // are the final admission authority and can select substantially less.
        let selected_limit = gpu_limit;
        let gpu_limit = if packed {
            gpu_limit
        } else {
            gpu_limit
                .min(capacity)
                .min(gpu_capacity.unwrap_or(usize::MAX))
        };
        self.effective = settings.splat_budget.min(gpu_limit);
        self.admission = selection::AdmissionSummary::default();
        let gpu_baseline = pinned
            .iter()
            .zip(gpu_costs)
            .filter(|(pin, _)| **pin)
            .fold(0usize, |sum, (_, cost)| sum.saturating_add(*cost));
        if baseline > capacity || gpu_capacity.is_some_and(|cap| gpu_baseline > cap) {
            self.last_error = Some("Coarsest SOG coverage exceeds source admission budget".into());
            return false;
        }
        let fixed_pins = pinned.clone();
        for range in self.active.iter().chain(&self.submitted) {
            pinned[range.file] = true;
        }
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.loading.is_some() {
                pinned[i] = true;
            }
        }
        let mut admissions = vec![selection::SourceAdmission {
            costs: &costs,
            pinned: &pinned,
            capacity,
        }];
        if let Some(capacity) = gpu_capacity {
            admissions.push(selection::SourceAdmission {
                costs: gpu_costs,
                pinned: &pinned,
                capacity,
            });
        }
        let initial = coarse.iter().any(|i| self.entries[*i].ready.is_none());
        let env_count = self
            .environment_index()
            .and_then(|i| self.entries[i].ready.as_ref())
            .map_or(0, |c| c.count);
        let coarse_count = self.manifest.leaves.iter().fold(0usize, |sum, leaf| {
            sum.saturating_add(leaf.lods.last().unwrap().1.count)
        });
        if coarse_count.saturating_add(env_count) > gpu_limit {
            self.last_error =
                Some("Coarsest SOG coverage exceeds the GPU storage binding limit".into());
            return false;
        }
        // Preserve the existing minimum-coverage floor, but never waive hard
        // source/device/slot ceilings. Report the actual upper bound honestly.
        self.effective = self.effective.max(coarse_count.saturating_add(env_count));
        let render_budget = settings
            .splat_budget
            // Choose ideal bands against the selected-slot/device ceiling.
            // Source-cache rows are not a second target splat budget: charge
            // exact file unions during admission instead. Conflating them here
            // can lower every target to coarse and discard useful resident detail
            // when one nearby file can never fit the source cache.
            .min(selected_limit)
            .saturating_sub(env_count);
        let reachable = next_files(&self.manifest, &self.entries);
        let mut desired = if initial {
            self.manifest
                .leaves
                .iter()
                .map(|l| Some(l.lods.last().unwrap().1))
                .collect()
        } else {
            selection::select_with_cap_evidence(
                &self.manifest,
                view,
                render_budget,
                &admissions,
                |i| {
                    reachable[i] && (self.entries[i].ready.is_some() || self.entries[i].can_retry())
                },
                self.diagnostics
                    .as_mut()
                    .map(|out| (out, diagnostic_projection)),
                &mut self.admission,
                Some(&mut self.overlay_intent.capped_leaves),
            )
        };
        if !initial && (self.admission.source_limited || self.relocation_files.is_some()) {
            let _timer = self.cpu_timings.start(CpuStage::RelocationPlan);
            let preferred = self.relocation_files.clone().unwrap_or_else(|| {
                let mut files = fixed_pins.clone();
                for range in &self.active {
                    files[range.file] = true;
                }
                files
            });
            // Desired ownership is independent of transport ownership, as in
            // PlayCanvas's pending replacement supersession. An obsolete load
            // keeps its byte reservation, but cannot freeze the camera's target.
            // This plans only the final set; transition_fits and live dispatch
            // below still charge outstanding loads and displayed generations.
            let preferred = relocation::plan_files(
                &self.manifest,
                view,
                render_budget,
                &admissions,
                &fixed_pins,
                &preferred,
                |i| {
                    // SOG files decode independently; a resident intermediate
                    // is not a dependency of the final destination.
                    self.entries[i].ready.is_some() || self.entries[i].can_retry()
                },
            );
            desired = relocation::select_files(
                &self.manifest,
                view,
                render_budget,
                &admissions,
                &preferred,
            );
            self.relocation_files = Some(preferred);
            // The bounded source planner can trade away a previously admissible
            // leaf for a more valuable one. Mark that source-set exclusion too.
            self.overlay_intent
                .capped_leaves
                .resize(self.manifest.leaves.len(), false);
            for (id, (leaf, target)) in self.manifest.leaves.iter().zip(&desired).enumerate() {
                if let Some(target) = target
                    && let Some((level, _)) = leaf.lods.iter().find(|(_, r)| r == target)
                    && leaf.lods.iter().any(|(lod, r)| {
                        *lod < *level
                            && self.entries[r.file].can_retry()
                            && self
                                .relocation_files
                                .as_ref()
                                .is_some_and(|files| !files[r.file])
                    })
                {
                    self.overlay_intent.capped_leaves[id] = true;
                }
            }
        }
        self.activation.observe(
            &self.cache_trace,
            self.generation,
            &self.manifest,
            &desired,
            &self.displayed,
            &self.entries,
        );
        self.cache_trace.selection(|| {
            let files = |ranges: &[SplatRange]| {
                let mut files: Vec<_> = ranges
                    .iter()
                    .filter(|r| r.count > 0)
                    .map(|r| r.file)
                    .collect();
                files.sort_unstable();
                files.dedup();
                files.truncate(256);
                files
            };
            crate::diagnostics::sog_cache::SelectionPlan {
                generation: self.generation,
                elapsed_ms: 0., // Set against the cache recorder's clock.
                eye: view.eye.to_array(),
                desired: files(&desired.iter().flatten().copied().collect::<Vec<_>>()),
                // Include both sides of an in-progress renderer handoff.
                active: files(
                    &self
                        .active
                        .iter()
                        .chain(&self.submitted)
                        .copied()
                        .collect::<Vec<_>>(),
                ),
                loading: self
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| e.loading.is_some().then_some(i))
                    .take(256)
                    .collect(),
                gpu_costs: gpu_costs.iter().copied().take(256).collect(),
                complete: self.entries.len() <= 256,
            }
        });
        self.overlay_intent.targets = if initial {
            desired.clone()
        } else {
            selection::target_ranges(&self.manifest, view, render_budget)
        };
        let queue = scheduling::requests(
            &self.manifest,
            &desired,
            &self.displayed,
            view,
            self.environment_index(),
        );
        let transition_fits = self.relocation_files.as_ref().is_none_or(|files| {
            let union: Vec<_> = files
                .iter()
                .zip(&pinned)
                .map(|(new, old)| *new || *old)
                .collect();
            relocation::fits(&union, &admissions)
        });
        // The destination fits by itself, but the old/new union may not. Submit
        // resident coarse coverage first; renderer readiness retires old pins.
        if !transition_fits
            && !self.entries.iter().any(|entry| entry.loading.is_some())
            && let Some(files) = &self.relocation_files
            && let Some(mut bridge) = self.cpu_timings.measure(CpuStage::RelocationPlan, || {
                relocation::covered_bridge(&self.manifest, &desired, files, &admissions, |i| {
                    self.entries[i].ready.is_some()
                })
            })
        {
            if let Some(environment) = self.environment_range() {
                bridge.push(environment);
            }
            // The bridge changes source owners, not the accepted destination.
            // Keep intent so renderer acknowledgement can unblock transport
            // without another camera evaluation. Progress rechecks the live
            // union, including old displayed pins, before any acquisition.
            self.replacements
                .reconcile(&self.manifest, &desired, &bridge);
            self.replacements.requests = queue.clone();
            self.replacements.dirty = true;
            if bridge != self.active {
                self.active = bridge;
                self.pending_work = true;
                self.last_error = None;
                if let Some(trace) = self.diagnostics.as_mut() {
                    for entry in trace {
                        let leaf = &self.manifest.leaves[entry.leaf];
                        let chosen = leaf
                            .lods
                            .iter()
                            .find(|(_, range)| self.active.contains(range));
                        entry.range = chosen.map(|(_, range)| *range);
                        entry.chosen_lod = chosen.and_then(|(lod, _)| u32::try_from(*lod).ok());
                        entry.reason = "releasing obsolete detail";
                    }
                }
                return true;
            }
        }
        if !transition_fits {
            // Future cold destinations need not fit alongside all current
            // owners for an independent ready leaf to advance. Retain intent
            // and check the actual resolved old/new union, not the full future
            // destination. This never dispatches requests or releases renderer
            // ownership; progress_requests still requires live headroom.
            self.replacements
                .reconcile(&self.manifest, &desired, &self.active);
            self.replacements.requests = queue;
            let changed = self.progress_replacements(settings, selected_limit, gpu_source_bytes);
            // A covered bridge must actually retire the old union before any
            // acquisition. Budget edits can also make outstanding reservations
            // temporarily exceed the new ceiling; let those finish first.
            self.last_error = Some("SOG relocation is waiting for source headroom".into());
            self.overlay_intent.transition_blocked = true;
            return changed;
        }
        let mut wanted = pinned.clone();
        for &i in &queue {
            wanted[i] = true;
        }
        for range in self.active.iter().chain(&self.submitted) {
            wanted[range.file] = true;
        }
        for &i in &queue {
            self.entries[i].last_used = self.generation;
        }
        let blocked = self.dispatch_requests(
            &queue, &costs, &wanted, capacity, env_cap, now, settings, &mut start,
        );
        self.overlay_intent.queued_targets = desired
            .iter()
            .map(|target| {
                target.filter(|range| {
                    queue.contains(&range.file)
                        && !blocked[range.file]
                        && (self.entries[range.file].loading.is_some()
                            || self.entries[range.file].eligible(now))
                })
            })
            .collect();
        // Availability can change independently of the desired detail. Each leaf falls back
        // to its best resident representation; never stack fine splats on coarse splats.
        // Requests started above are already reservations for this pass too.
        drop(admissions);
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.loading.is_some() {
                pinned[i] = true;
            }
        }
        let mut admissions = vec![selection::SourceAdmission {
            costs: &costs,
            pinned: &pinned,
            capacity,
        }];
        if let Some(capacity) = gpu_capacity {
            admissions.push(selection::SourceAdmission {
                costs: gpu_costs,
                pinned: &pinned,
                capacity,
            });
        }
        let mut resident_trace = self.diagnostics.as_ref().map(|_| Vec::new());
        let mut resident_admission = selection::AdmissionSummary::default();
        let mut resident_caps = Vec::new();
        let mut selected: Vec<_> = selection::select_with_cap_evidence(
            &self.manifest,
            view,
            render_budget,
            &admissions,
            |i| {
                self.entries[i].ready.is_some()
                    && self.relocation_files.as_ref().is_none_or(|files| files[i])
            },
            resident_trace
                .as_mut()
                .map(|out| (out, diagnostic_projection)),
            &mut resident_admission,
            Some(&mut resident_caps),
        )
        .into_iter()
        .flatten()
        .filter(|r| r.count > 0)
        .collect();
        // Resolve downgrades before spending their count on other upgrades.
        // Old source owners are already charged in `pinned`; covered pressure
        // bridges above remain the explicit exception to old-detail retention.
        selected = replacement::Replacements::policy_selection(
            &self.manifest,
            &desired,
            &self.active,
            selected,
            render_budget.max(coarse_count),
            |file| self.entries[file].ready.is_some(),
        );
        if let Some(trace) = resident_trace.as_mut() {
            let chosen: HashSet<_> = selected
                .iter()
                .map(|range| (range.file, range.offset, range.count))
                .collect();
            for (leaf, entry) in self.manifest.leaves.iter().zip(trace) {
                let range = leaf
                    .lods
                    .iter()
                    .find(|(_, range)| chosen.contains(&(range.file, range.offset, range.count)));
                let actual = range.map(|(_, range)| *range);
                if entry.range != actual {
                    entry.range = actual;
                    entry.chosen_lod = range.and_then(|(lod, _)| u32::try_from(*lod).ok());
                    entry.reason = "waiting for replacement";
                }
            }
        }
        // New downloads became immutable pins between passes. Resident-only
        // rejection must remain visible even when leaf tracing is disabled.
        self.admission.source_limited |= resident_admission.source_limited;
        self.admission.selection_limited |= resident_admission.selection_limited;
        self.overlay_intent
            .capped_leaves
            .resize(resident_caps.len(), false);
        for (cap, resident_cap) in self
            .overlay_intent
            .capped_leaves
            .iter_mut()
            .zip(resident_caps)
        {
            *cap |= resident_cap;
        }
        if let Some(mut resident_trace) = resident_trace {
            if !initial && let Some(desired_trace) = &self.diagnostics {
                for (resident, desired) in resident_trace.iter_mut().zip(desired_trace) {
                    if matches!(desired.reason, "source transition" | "device/slot cap") {
                        resident.reason = desired.reason;
                    }
                }
            }
            self.diagnostics = Some(resident_trace);
        }
        if let Some(environment) = self.environment_range() {
            selected.push(environment);
        }
        self.pending_work = self.entries.iter().any(|e| e.loading.is_some())
            || queue.iter().any(|i| {
                !blocked[*i] && self.entries[*i].ready.is_none() && self.entries[*i].can_retry()
            });
        if blocked.iter().any(|blocked| *blocked) {
            self.last_error = Some("SOG source transition exceeds the decoded cache budget".into());
        }
        self.overlay_intent.blocked_files = blocked.clone();
        if !self.pending_work
            && !blocked.iter().any(|blocked| *blocked)
            && !queue
                .iter()
                .any(|i| self.entries[*i].ready.is_none() && self.entries[*i].attempts >= 3)
        {
            self.last_error = None;
        }
        let changed = selected != self.active;
        self.replacements
            .reconcile(&self.manifest, &desired, &selected);
        self.replacements.requests = queue;
        if changed {
            self.active = selected;
        }
        changed
    }
}

/// Only expose the next finer level after a resident level, as in upstream's
/// prefetchNextLod. Shared files are deduplicated across leaves.
pub(super) fn next_files(manifest: &Manifest, entries: &[Entry]) -> Vec<bool> {
    let mut files = vec![false; manifest.filenames.len()];
    for leaf in &manifest.leaves {
        let current = leaf
            .lods
            .iter()
            .position(|(_, r)| r.count == 0 || entries[r.file].ready.is_some())
            .unwrap_or(leaf.lods.len() - 1);
        for (_, r) in &leaf.lods[current..] {
            files[r.file] = true;
        }
        if let Some((_, r)) = leaf.lods[..current]
            .iter()
            .rev()
            .find(|(_, r)| entries[r.file].can_retry())
        {
            files[r.file] = true;
        }
    }
    files
}
