//! Advance admitted transport without running camera or LOD policy.
use super::*;
use crate::diagnostics::sog_cache::ActivationGuard;

impl SogStream {
    /// Invalidate only on changed live inputs. A failed check consumes `dirty`,
    /// so unchanged pressure does not re-run leaf resolution every frame.
    pub(super) fn refresh_replacement_admission(
        &mut self,
        settings: &SogLodSettings,
        gpu_limit: usize,
        gpu_bytes: Option<u64>,
    ) {
        let limits = (
            settings.cache_bytes,
            settings.splat_budget,
            gpu_limit,
            gpu_bytes,
        );
        if self.replacement_limits != Some(limits) {
            self.replacement_limits = Some(limits);
            self.replacements.dirty |= !self.replacements.leaves.is_empty();
        }
    }

    /// Live accounting, not a reservation copied from the last policy pass.
    /// GPU descriptors have tiny CPU cost but retain their full GPU charge.
    fn request_costs(&self, settings: &SogLodSettings) -> (usize, usize, Vec<usize>, Vec<usize>) {
        let packed = self.storage == SourceStorage::Packed;
        let capacity = if packed {
            settings.cache_bytes
        } else {
            settings.cache_bytes / chunk::DecodedChunk::bytes_per_splat()
        };
        let env_cap = if self.environment_index().is_some() {
            capacity.min(100_000)
        } else {
            0
        };
        let mut cpu = Vec::with_capacity(self.entries.len());
        let mut gpu = Vec::with_capacity(self.entries.len());
        for (i, entry) in self.entries.iter().enumerate() {
            let count = entry.ready.as_ref().map_or_else(
                || {
                    entry.reserved_splats.unwrap_or_else(|| {
                        self.manifest.file_counts.get(i).copied().unwrap_or(env_cap)
                    })
                },
                |chunk| chunk.count,
            );
            cpu.push(entry.ready.as_ref().map_or_else(
                || {
                    if packed {
                        packed_reservation(count)
                    } else {
                        cache::reserved_rows(count)
                    }
                },
                |chunk| {
                    if packed {
                        chunk.cloud.retained_bytes() as usize
                    } else {
                        chunk.padded_rows()
                    }
                },
            ));
            gpu.push(if packed {
                entry.ready.as_ref().map_or_else(
                    || packed_reservation(count),
                    |chunk| chunk.cloud.admission_bytes() as usize,
                )
            } else {
                count
            });
        }
        (capacity, env_cap, cpu, gpu)
    }

    fn owned_files(&self) -> Vec<bool> {
        let mut files = vec![false; self.entries.len()];
        for file in self
            .manifest
            .coarse_files()
            .into_iter()
            .chain(self.environment_index())
            .chain(
                self.active
                    .iter()
                    .chain(&self.submitted)
                    .chain(&self.displayed)
                    .map(|range| range.file),
            )
        {
            files[file] = true;
        }
        for (file, entry) in self.entries.iter().enumerate() {
            files[file] |= entry.loading.is_some();
        }
        files
    }

    fn union_fits(
        &self,
        files: &[bool],
        cpu: &[usize],
        gpu: &[usize],
        capacity: usize,
        gpu_bytes: Option<u64>,
    ) -> bool {
        let used = |costs: &[usize]| {
            files
                .iter()
                .zip(costs)
                .filter(|(used, _)| **used)
                .fold(0usize, |sum, (_, cost)| sum.saturating_add(*cost))
        };
        used(cpu) <= capacity
            && gpu_bytes.is_none_or(|bytes| {
                let limit = if self.storage == SourceStorage::Packed {
                    bytes
                } else {
                    bytes / renderer::source_bytes_per_splat()
                };
                used(gpu) as u64 <= limit
            })
    }

    /// Refill slots from the latest policy's queue; do not run selection or
    /// choose destinations here. Obsolete in-flight requests keep reservations.
    pub(super) fn progress_requests(
        &mut self,
        now: f64,
        settings: &SogLodSettings,
        gpu_bytes: Option<u64>,
        start: impl FnMut(String, Option<usize>, usize, f64) -> ChunkLoad,
    ) {
        if self.replacements.requests.is_empty() {
            return;
        }
        let slot_free = self
            .entries
            .iter()
            .filter(|entry| entry.loading.is_some())
            .count()
            < settings.concurrent_chunks.clamp(1, 4);
        if !self.replacements.requests.iter().any(|file| {
            let entry = &self.entries[*file];
            entry.eligible(now) && (slot_free || entry.warm.is_some())
        }) {
            return;
        }
        let (capacity, env_cap, cpu, gpu) = self.request_costs(settings);
        let mut wanted = self.owned_files();
        for &file in &self.replacements.requests {
            wanted[file] = true;
        }
        // A smaller cap or another scene's new pins can revoke headroom. A
        // retained queue is intent, never permission to bypass current limits.
        if !self.union_fits(&wanted, &cpu, &gpu, capacity, gpu_bytes) {
            return;
        }
        let queue = self.replacements.requests.clone();
        let blocked = self.dispatch_requests(
            &queue, &cpu, &wanted, capacity, env_cap, now, settings, start,
        );
        self.pending_work = self.entries.iter().any(|entry| entry.loading.is_some())
            || queue.iter().any(|file| {
                !blocked[*file]
                    && self.entries[*file].ready.is_none()
                    && self.entries[*file].can_retry()
            });
        self.overlay_intent.blocked_files = blocked;
    }

    /// Resolve leaf transitions only after the previous generation is ready.
    /// Also resolve ready accepted targets at the start of a due policy tick,
    /// before that policy supersedes intent. This never dispatches old requests.
    pub(super) fn progress_replacements(
        &mut self,
        settings: &SogLodSettings,
        gpu_limit: usize,
        gpu_bytes: Option<u64>,
    ) -> bool {
        self.replacements.dirty = false;
        let env_count = self.environment_range().map_or(0, |range| range.count);
        let floor = self
            .manifest
            .leaves
            .iter()
            .map(|leaf| leaf.lods.last().unwrap().1.count)
            .fold(env_count, usize::saturating_add);
        let budget = settings
            .splat_budget
            .max(floor)
            .min(gpu_limit)
            .saturating_sub(env_count);
        let mut candidate = self
            .replacements
            .resolve(budget, |file| self.entries[file].ready.is_some());
        if let Some(environment) = self.environment_range() {
            candidate.push(environment);
        }
        if candidate == self.active {
            self.trace_guard(ActivationGuard::ReplacementUnchanged);
            return false;
        }
        if candidate
            .iter()
            .fold(0usize, |sum, range| sum.saturating_add(range.count))
            > budget.saturating_add(env_count)
        {
            self.trace_guard(ActivationGuard::CountAdmission);
            return false;
        }
        let (capacity, _, cpu, gpu) = self.request_costs(settings);
        let mut union = self.owned_files();
        for range in &candidate {
            union[range.file] = true;
        }
        if !self.union_fits(&union, &cpu, &gpu, capacity, gpu_bytes) {
            self.trace_guard(ActivationGuard::SourceAdmission);
            return false;
        }
        self.active = candidate;
        // The next completion starts from this candidate, not a retired old
        // range. Actual lifetime remains with active/submitted/displayed owners.
        let targets: Vec<_> = self
            .replacements
            .leaves
            .iter()
            .map(|leaf| leaf.target)
            .collect();
        self.replacements
            .reconcile(&self.manifest, &targets, &self.active);
        true
    }

    /// Start eligible requests within source and concurrency reservations.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dispatch_requests(
        &mut self,
        queue: &[usize],
        costs: &[usize],
        wanted: &[bool],
        capacity: usize,
        env_cap: usize,
        now: f64,
        settings: &SogLodSettings,
        mut start: impl FnMut(String, Option<usize>, usize, f64) -> ChunkLoad,
    ) -> Vec<bool> {
        let packed = self.storage == SourceStorage::Packed;
        let mut in_flight = self.entries.iter().filter(|e| e.loading.is_some()).count();
        let mut blocked = vec![false; self.entries.len()];
        for &i in queue {
            if !self.entries[i].eligible(now) {
                continue;
            }
            // Warm hits consume no download slot. Keep scanning past a cold
            // request when slots are full so later cached requests can proceed.
            if self.entries[i].warm.is_none() && in_flight >= settings.concurrent_chunks.clamp(1, 4)
            {
                continue;
            }
            if !cache::admit_observed(
                &mut self.entries,
                costs,
                wanted,
                i,
                capacity,
                |file, bytes| {
                    self.cache_trace.record(
                        crate::diagnostics::sog_cache::Kind::CpuAdmissionEviction,
                        file,
                        bytes,
                        Some(i),
                        self.generation,
                    );
                },
            ) {
                blocked[i] = true;
                continue;
            }
            if packed && warm_cache::restore(&mut self.entries[i]) {
                self.replacements.dirty |= self.replacements.wants(i);
                self.activation.source_ready(&self.cache_trace, i);
                self.cache_trace.record(
                    crate::diagnostics::sog_cache::Kind::WarmReuse,
                    i,
                    self.entries[i]
                        .ready
                        .as_ref()
                        .unwrap()
                        .cloud
                        .retained_bytes(),
                    None,
                    self.generation,
                );
                warm_cache::trim(&mut self.entries, capacity, 0, |file, bytes| {
                    self.cache_trace.record(
                        crate::diagnostics::sog_cache::Kind::WarmEviction,
                        file,
                        bytes,
                        Some(i),
                        self.generation,
                    );
                });
                continue;
            }
            let (path, expected, max) = if i < self.manifest.filenames.len() {
                (
                    &self.manifest.filenames[i],
                    Some(self.manifest.file_counts[i]),
                    self.manifest.file_counts[i],
                )
            } else {
                (self.manifest.environment.as_ref().unwrap(), None, env_cap)
            };
            if packed {
                // Warm copies are not policy pins. Reclaim them before the new
                // immutable reservation can start downloading/decoding.
                warm_cache::trim(&mut self.entries, capacity, costs[i], |file, bytes| {
                    self.cache_trace.record(
                        crate::diagnostics::sog_cache::Kind::WarmEviction,
                        file,
                        bytes,
                        Some(i),
                        self.generation,
                    );
                });
            }
            use crate::diagnostics::sog_cache::Kind;
            let kind = if self.entries[i].attempts > 0 {
                Kind::Retry
            } else if self.entries[i].ever_decoded {
                Kind::Reload
            } else {
                Kind::Request
            };
            self.cache_trace.record(kind, i, 0, None, self.generation);
            self.entries[i].attempts += 1;
            self.entries[i].reserved_splats = Some(expected.unwrap_or(max));
            self.entries[i].loading = Some(start(
                format!("{}{path}", self.base_url),
                expected,
                max,
                now,
            ));
            in_flight += 1;
        }
        blocked
    }
}
