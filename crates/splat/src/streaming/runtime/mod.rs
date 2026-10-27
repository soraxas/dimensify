//! Progressive streamed-SOG loading. HTTP and cache policy stay outside the renderer.

mod activation;
mod cache;
mod chunk;
pub mod diagnostics;
mod distance;
mod manifest;
pub mod overlay;
mod pages;
mod progress;
mod relocation;
mod replacement;
mod requests;
mod scheduling;
mod screen_space;
mod selection;
mod warm_cache;

use crate::assets::url_directory;
use crate::diagnostics::sog_cpu::{CpuStage, SogCpuTimings};
use crate::renderer::{self, SnapshotRenderer, SnapshotState};
use bevy::prelude::*;
use cache::Entry;
#[cfg(target_arch = "wasm32")]
pub mod codec;
use chunk::ChunkLoad;
use manifest::{Manifest, SplatRange};
use progress_types::{StreamProgress as ProgressTask, StreamProgressState as ProgressTaskState};
use renderer::source::{SourceStorage, packed_reservation};
mod progress_types;
pub use progress_types::{StreamProgress, StreamProgressState};

/// Camera used for streamed selection; consumers explicitly select their view.
#[derive(Component)]
pub struct StreamingCamera;
use std::collections::HashSet;

pub use crate::streaming::{SogLodSettings, SogVisibilityPolicy};

#[derive(Component)]
pub struct SogStream {
    manifest: Manifest,
    resolver: crate::assets::AssetResolver,
    base_url: String,
    pub(crate) name: String,
    pub progress_id: String,
    entries: Vec<Entry>,
    active: Vec<SplatRange>,
    /// Last successfully accepted selection, including a pending generation.
    submitted: Vec<SplatRange>,
    /// Render-ready generation, distinct from accepted or requested detail.
    displayed: Vec<SplatRange>,
    /// Lightweight debug evidence from the actual scheduler (no leaf text trace).
    overlay_intent: overlay::RefinementIntent,
    quality_history: Vec<usize>,
    quality_config: Option<(crate::streaming::SogLodPolicy, u32)>,
    submission_error: Option<String>,
    snapshot: SnapshotState,
    pending_work: bool,
    /// Active ranges and their CPU owners stay pinned during bounded uploads.
    preparing_upload: bool,
    next_update: f64,
    generation: u64,
    last_error: Option<String>,
    /// Task 6 opts in by setting Some; normal streaming allocates no leaf trace.
    pub diagnostics: Option<Vec<diagnostics::SogLeafDiagnostic>>,
    requested: usize,
    effective: usize,
    rejected_admission: Option<(Vec<SplatRange>, usize, Option<u64>)>,
    admission: selection::AdmissionSummary,
    /// Keep replacement intent through the covered-release/download stages.
    /// Otherwise resident old files can immediately win selection again.
    relocation_files: Option<Vec<bool>>,
    replacements: replacement::Replacements,
    /// Last observed live limits, not reserved permission to admit a source.
    replacement_limits: Option<(usize, usize, usize, Option<u64>)>,
    cpu_timings: SogCpuTimings,
    cache_trace: crate::diagnostics::sog_cache::SceneTrace,
    activation: activation::Tracker,
    storage: SourceStorage,
    pages: Option<pages::PageCache>,
    preparing_pages: bool,
}

/// Read-only capture/UI evidence; selection and transport remain stream-owned.
pub struct SogStreamStatus {
    pub evaluated: bool,
    pub requested: usize,
    pub effective: usize,
    pub selected: usize,
    pub displayed: usize,
    pub source_limited: bool,
    pub selection_limited: bool,
    pub pending_downloads: usize,
    pub pending_generation: bool,
    pub pending_work: bool,
    pub limiting_reason: String,
    pub cache_ready_bytes: u64,
    pub cache_reserved_bytes: u64,
}

impl SogStream {
    /// Use application-authorized URL or byte resolution for this scene.
    pub fn with_resolver(mut self, resolver: crate::assets::AssetResolver) -> Self {
        self.resolver = resolver;
        self
    }

    /// Bounded GPU copies need the next display frame; network-only waits do not.
    pub fn needs_upload_frame(&self) -> bool {
        self.preparing_upload || self.preparing_pages
    }

    /// Navigation/fitting use the complete authored scene, not a streamed
    /// generation (or the environment backdrop). The root transform owns the
    /// SOG-to-world conversion, exactly as it does for rendered children.
    pub fn fit_extent(&self) -> dimensify_camera::FitExtent {
        dimensify_camera::FitExtent::from_min_max(self.manifest.bound.min, self.manifest.bound.max)
    }

    /// Enable bounded leaf evidence without changing generation ownership.
    pub fn configure_diagnostics(&mut self, enabled: bool) {
        if enabled && self.diagnostics.is_none() {
            self.diagnostics = Some(Vec::new());
        }
        if !enabled {
            self.diagnostics = None;
        }
        // A UI intent must take effect in this frame, even when idle streaming
        // was throttled. Pending GPU generation ownership is unchanged.
        self.next_update = 0.0;
    }
    /// Validate a streamed hierarchy and initialize its scene-owned controller.
    pub fn parse(
        bytes: &[u8],
        url: &str,
        name: String,
        progress_id: String,
    ) -> Result<Self, String> {
        let manifest = Manifest::parse(bytes)?;
        let count = manifest.filenames.len() + usize::from(manifest.environment.is_some());
        Ok(Self {
            manifest,
            resolver: default(),
            base_url: url_directory(url).into(),
            name,
            progress_id,
            entries: (0..count).map(|_| Entry::default()).collect(),
            active: Vec::new(),
            submitted: Vec::new(),
            displayed: Vec::new(),
            overlay_intent: default(),
            quality_history: Vec::new(),
            quality_config: None,
            submission_error: None,
            snapshot: SnapshotState::default(),
            pending_work: false,
            preparing_upload: false,
            next_update: 0.0,
            generation: 0,
            last_error: None,
            diagnostics: None,
            requested: 0,
            effective: 0,
            rejected_admission: None,
            admission: selection::AdmissionSummary::default(),
            relocation_files: None,
            replacements: default(),
            replacement_limits: None,
            cpu_timings: SogCpuTimings::default(),
            cache_trace: default(),
            activation: default(),
            storage: SourceStorage::Expanded,
            pages: None,
            preparing_pages: false,
        })
    }

    /// Read selection, readiness and memory observations without changing policy.
    pub fn status(&self) -> SogStreamStatus {
        let (cache_ready_bytes, cache_reserved_bytes, _) = self.cache_memory();
        SogStreamStatus {
            evaluated: self.generation > 0,
            cache_ready_bytes,
            cache_reserved_bytes,
            requested: self.requested,
            effective: self.effective,
            selected: self
                .submitted
                .iter()
                .fold(0usize, |n, r| n.saturating_add(r.count)),
            displayed: self
                .displayed
                .iter()
                .fold(0usize, |n, r| n.saturating_add(r.count)),
            source_limited: self.admission.source_limited,
            selection_limited: self.admission.selection_limited,
            pending_downloads: self.entries.iter().filter(|e| e.loading.is_some()).count(),
            pending_generation: self.snapshot.is_pending() || self.needs_upload_frame(),
            pending_work: self.pending_work,
            limiting_reason: self
                .submission_error
                .as_ref()
                .or(self.last_error.as_ref())
                .cloned()
                .unwrap_or_else(|| {
                    if self.generation == 0 {
                        return "Awaiting first selection evaluation".into();
                    }
                    let mut reasons = Vec::new();
                    if self.effective > self.requested {
                        reasons.push(
                            "minimum coverage exceeds requested target (coarse coverage floor)",
                        );
                    }
                    if self.admission.source_limited {
                        reasons.push(if self.pages.is_some() {
                            "page source-transition candidates rejected"
                        } else {
                            "whole-file source-transition candidates rejected"
                        });
                    }
                    if self.admission.selection_limited {
                        reasons.push("candidates exceeded selection budget");
                    }
                    if self.snapshot.is_pending() {
                        reasons.push("GPU generation pending (accepted is not visible count)");
                    }
                    if self.pending_work {
                        reasons.push("loading/refining");
                    }
                    if reasons.is_empty() {
                        reasons.push("available LOD selected; cap is not guaranteed refinement");
                    }
                    reasons.join("; ")
                }),
        }
    }

    /// Report old and pending snapshot allocation estimates.
    pub fn snapshot_memory(&self, cameras: usize) -> renderer::SelectionMemory {
        self.snapshot.memory(cameras)
    }

    /// CPU payload accounting: ready (including warm), reserved, warm subset.
    /// No GPU ownership or process-RSS claim is implied by these byte totals.
    pub fn cache_memory(&self) -> (u64, u64, u64) {
        let mut ready = self
            .pages
            .as_ref()
            .map_or(0, |pages| pages.resident_bytes());
        let mut reserved = 0u64;
        let mut warm = 0u64;
        for entry in &self.entries {
            let cached = entry
                .warm
                .as_ref()
                .map_or(0, |chunk| chunk.cloud.retained_bytes());
            warm = warm.saturating_add(cached);
            ready = ready.saturating_add(cached).saturating_add(
                entry
                    .ready
                    .as_ref()
                    .map_or(0, |chunk| chunk.cloud.retained_bytes()),
            );
            if entry.loading.is_some() {
                let rows = entry.reserved_splats.unwrap_or(0);
                reserved = reserved.saturating_add(match self.storage {
                    SourceStorage::Packed => packed_reservation(rows) as u64,
                    SourceStorage::Expanded => (cache::reserved_rows(rows) as u64)
                        .saturating_mul(renderer::source_bytes_per_splat()),
                });
            }
        }
        (ready, reserved, warm)
    }

    fn environment_index(&self) -> Option<usize> {
        self.manifest
            .environment
            .as_ref()
            .map(|_| self.manifest.filenames.len())
    }

    /// Only decoded, nonempty environment data contributes renderable coverage.
    fn environment_range(&self) -> Option<SplatRange> {
        let file = self.environment_index()?;
        let count = if let Some(pages) = &self.pages {
            let layout = pages.layouts.get(file)?;
            if !layout.known {
                return None;
            }
            let range = SplatRange {
                file,
                offset: 0,
                count: layout.count,
            };
            if !pages.covers(range) {
                return None;
            }
            layout.count
        } else {
            self.entries[file].ready.as_ref()?.count
        };
        (count > 0).then_some(SplatRange {
            file,
            offset: 0,
            count,
        })
    }

    fn tick(
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
fn next_files(manifest: &Manifest, entries: &[Entry]) -> Vec<bool> {
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

/// Initialize the shared streaming admission and diagnostic resources.
pub fn install_sog_streaming(app: &mut App) {
    renderer::readiness::install(app);
}

impl SogStream {
    /// Reconcile renderer residency without giving cache entries ownership of
    /// the currently displayed scene. The callback owns GPU truth.
    fn sync_resident_sources(
        &mut self,
        mut synchronize: impl FnMut(usize, &mut Option<renderer::DecodedChunk>) -> bool,
    ) {
        let mut invalidated = false;
        for (file, entry) in self.entries.iter_mut().enumerate() {
            if warm_cache::sync(entry, |ready| synchronize(file, ready)) {
                // Actual GPU eviction is not a network failure.
                entry.attempts = 0;
                entry.retry_at = 0.;
                // Dropping an unused cache entry invalidates no owned coverage.
                // Reconsider future eligibility at the normal policy deadline;
                // otherwise routine admission eviction feeds back into
                // immediate live-camera replanning, bypassing policy cadence.
                // Keep prompt recovery for any generation owner or required
                // coverage; renderer retirement/admission remains authoritative.
                invalidated |= self
                    .active
                    .iter()
                    .chain(&self.submitted)
                    .chain(&self.displayed)
                    .any(|range| range.file == file)
                    || self.manifest.leaves.iter().any(|leaf| {
                        leaf.lods
                            .last()
                            .is_some_and(|(_, range)| range.file == file)
                    })
                    || (self.manifest.environment.is_some()
                        && file == self.manifest.filenames.len());
            }
        }
        if invalidated {
            self.next_update = 0.;
        }
    }

    /// Transport/worker progress is independent of policy evaluation and GPU
    /// readiness. Keep completion admission bounded to one chunk per frame;
    /// the worker's expanded-data transfer already yields in bounded batches.
    fn poll_loads(&mut self, now: f64) {
        let mut completed = None;
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if let Some(load) = entry.loading.as_mut()
                && let Some(result) = load.poll_measured(now, &self.cpu_timings)
            {
                let (kind, bytes) = match &result {
                    Ok(chunk) => (
                        crate::diagnostics::sog_cache::Kind::Decoded,
                        chunk.cloud.retained_bytes(),
                    ),
                    Err(_) => (crate::diagnostics::sog_cache::Kind::DecodeFailed, 0),
                };
                self.cache_trace
                    .record(kind, i, bytes, None, self.generation);
                if let Some(error) = entry.complete(result, now) {
                    warn!("SOG chunk {i}: {error}");
                    self.last_error = Some(error);
                } else {
                    completed = Some(i);
                    self.activation.source_ready(&self.cache_trace, i);
                }
                // Do not clear pending_work: the new source still needs policy
                // evaluation and renderer submission at the next deadline.
                self.pending_work = true;
                self.preparing_pages = self.pages.is_some();
                break;
            }
        }
        // Wake selection for useful newly available data, not merely for
        // camera motion, failed attempts, or obsolete downloads. The outer
        // loop still admits at most one completion this frame and evaluates
        // on the next frame. GPU back pressure must retain this due tick.
        if let Some(file) = completed
            && self.pages.is_none()
            && self.replacements.wants(file)
        {
            self.replacements.dirty = true;
        } else if let Some(file) = completed
            && (self.environment_index() == Some(file)
                || self.manifest.coarse_files().contains(&file)
                || self
                    .overlay_intent
                    .queued_targets
                    .iter()
                    .flatten()
                    .any(|range| range.file == file))
        {
            self.next_update = self.next_update.min(now);
        }
    }
}

/// Camera positions and scene transforms are read after transform propagation.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_sog_streams(
    mut renderer: SnapshotRenderer,
    time: Res<Time<Real>>,
    settings: Res<SogLodSettings>,
    mut streams: Query<(Entity, &GlobalTransform, &mut SogStream)>,
    cameras: Query<
        (Entity, &GlobalTransform, &Projection, &Camera),
        (With<StreamingCamera>, With<Camera3d>),
    >,
) {
    let Ok((camera, camera_transform, projection, camera_component)) = cameras.single() else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let gpu_limit = renderer.splat_limit();
    for (entity, transform, mut stream) in &mut streams {
        let resolver = stream.resolver.clone();
        if stream.generation == 0 {
            stream.storage = renderer.source_storage();
            stream.activation.storage = stream.storage;
            if renderer.uses_pages()
                && stream.pages.is_none()
                && let Err(error) = stream.enable_pages()
            {
                stream.last_error = Some(error);
                continue;
            }
        }
        stream.cpu_timings = renderer.cpu_timings();
        stream.cache_trace = renderer.cache_trace(entity);
        if stream.pages.is_none() && stream.storage == SourceStorage::Packed {
            let trace = stream.cache_trace.clone();
            let generation = stream.generation;
            stream.sync_resident_sources(|file, ready| renderer.sync_source(entity, file, ready));
            warm_cache::trim(
                &mut stream.entries,
                settings.cache_bytes,
                0,
                |file, bytes| {
                    trace.record(
                        crate::diagnostics::sog_cache::Kind::WarmEviction,
                        file,
                        bytes,
                        None,
                        generation,
                    );
                },
            );
        }
        let ready = renderer.poll(&mut stream.snapshot);
        if ready && stream.snapshot.is_visible() && stream.displayed != stream.submitted {
            stream.displayed = stream.submitted.clone();
            // Releasing old displayed ownership can make a previously rejected
            // replacement union fit without another download or policy tick.
            stream.replacements.dirty |= !stream.replacements.leaves.is_empty();
            let SogStream {
                activation,
                cache_trace,
                displayed,
                ..
            } = &mut *stream;
            activation.activated(cache_trace, displayed);
        }
        let source_capacity = renderer.source_cache_bytes(entity);
        stream.refresh_replacement_admission(&settings, gpu_limit, source_capacity);
        let progress_only = now < stream.next_update && !stream.needs_upload_frame();
        if progress_only {
            stream.poll_loads(now);
            // Bootstrap/environment/page completions retain their policy wake.
            if stream.pages.is_some() || stream.next_update <= now {
                continue;
            }
            let storage = stream.storage;
            stream.progress_requests(
                now,
                &settings,
                source_capacity,
                |url, expected, max, now| {
                    ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                        .with_owner(entity.to_bits())
                },
            );
            if !ready {
                stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::RendererPending);
                continue;
            }
            if stream.submission_error.is_some() {
                stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::SubmissionError);
                continue;
            }
            if !stream.replacements.dirty {
                stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::PolicyWait);
                continue;
            }
            if !stream.progress_replacements(&settings, gpu_limit, source_capacity) {
                continue;
            }
        }
        if !ready {
            stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::RendererPending);
            // Keep the due tick latched, but do no pin rebuilding or camera
            // math while renderer ownership prevents using that selection.
            // Turning away must not discard the previous complete coverage.
            stream.poll_loads(now);
            stream.progress_pages(settings.cache_bytes);
            if stream.pages.is_none() {
                let storage = stream.storage;
                stream.progress_requests(
                    now,
                    &settings,
                    source_capacity,
                    |url, expected, max, now| {
                        ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                            .with_owner(entity.to_bits())
                    },
                );
            }
            continue;
        }
        if stream.pages.is_none() {
            renderer.pin_sources(
                entity,
                stream
                    .manifest
                    .coarse_files()
                    .into_iter()
                    .chain(stream.environment_index()),
            );
        }
        let eye = transform
            .affine()
            .inverse()
            .transform_point3(camera_transform.translation());
        let eye = if eye.is_finite() { eye } else { Vec3::ZERO };
        let forward = transform
            .affine()
            .inverse()
            .transform_vector3(*camera_transform.forward())
            .normalize_or_zero();
        let fov_scale = match projection {
            Projection::Perspective(p) => {
                let tan_v = (p.fov * 0.5).tan();
                tan_v.min(tan_v * p.aspect_ratio) / (std::f32::consts::FRAC_PI_8).tan()
            }
            _ => 1.0,
        };
        let clip_from_local = if settings.effective_visibility() == SogVisibilityPolicy::Frustum
            || stream.diagnostics.is_some()
        {
            Some(
                projection.get_clip_from_view()
                    * camera_transform.to_matrix().inverse()
                    * transform.to_matrix(),
            )
        } else {
            None
        };
        let view = selection::View {
            pixels: camera_component
                .physical_viewport_size()
                .map(|size| screen_space::PixelView {
                    clip_from_local: projection.get_clip_from_view()
                        * camera_transform.to_matrix().inverse()
                        * transform.to_matrix(),
                    viewport: size.as_vec2(),
                }),
            eye,
            forward,
            fov_scale,
            clip_from_local,
            ..Vec3::ZERO.into()
        };
        // A rejected policy candidate never owned a renderer generation. Only
        // the last accepted selection must remain pinned on the next attempt.
        if stream.submission_error.is_some() && !stream.preparing_upload {
            stream.active = stream.submitted.clone();
        }
        let storage = stream.storage;
        if !stream.preparing_upload && !progress_only {
            stream.tick(
                view,
                now,
                &settings,
                gpu_limit,
                source_capacity,
                |url, expected, max, now| {
                    ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                        .with_owner(entity.to_bits())
                },
            );
            // Advance only after a real evaluation. A pending GPU generation
            // defers the due tick; polling it must not consume another 200ms.
            stream.next_update = now + 0.2;
        } else if !progress_only {
            stream.poll_loads(now);
            stream.progress_pages(settings.cache_bytes);
            if stream.pages.is_none() {
                stream.progress_requests(
                    now,
                    &settings,
                    source_capacity,
                    |url, expected, max, now| {
                        ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                            .with_owner(entity.to_bits())
                    },
                );
            }
        }
        if stream.active != stream.submitted {
            if stream.active.is_empty() {
                renderer.clear(&mut stream.snapshot);
                stream.submitted.clear();
                stream.displayed.clear();
                stream.submission_error = None;
                stream.rejected_admission = None;
            } else if stream
                .rejected_admission
                .as_ref()
                .is_none_or(|(ranges, limit, source)| {
                    ranges != &stream.active || *limit != gpu_limit || *source != source_capacity
                })
            {
                let SogStream {
                    snapshot,
                    active,
                    entries,
                    pages,
                    ..
                } = &mut *stream;
                let result = if let Some(pages) = pages {
                    pages.split(active).and_then(|ranges| {
                        renderer.submit(snapshot, entity, camera, &ranges, |id| pages.source(id))
                    })
                } else {
                    renderer.submit(snapshot, entity, camera, active, |file| {
                        entries.get(file).and_then(|entry| entry.ready.as_ref())
                    })
                };
                match result {
                    Ok(renderer::Submission::Preparing) => {
                        stream.preparing_upload = true;
                        stream.submission_error = None;
                        stream.trace_guard(
                            crate::diagnostics::sog_cache::ActivationGuard::UploadPreparing,
                        );
                    }
                    Ok(renderer::Submission::Submitted) => {
                        stream.preparing_upload = false;
                        let count: usize = stream.active.iter().map(|r| r.count).sum();
                        debug!(
                            "SOG LOD '{}': {count} selected splats in {} ranges",
                            stream.name,
                            stream.active.len()
                        );
                        stream.submitted = stream.active.clone();
                        let SogStream {
                            activation,
                            cache_trace,
                            submitted,
                            ..
                        } = &mut *stream;
                        activation.submitted(cache_trace, submitted);
                        stream.submission_error = None;
                        stream.rejected_admission = None;
                    }
                    Err(error) => {
                        stream.preparing_upload = false;
                        if stream.submission_error.as_ref() != Some(&error) {
                            warn!("SOG submission: {error}");
                        }
                        if storage != SourceStorage::Packed
                            && (error.contains("GPU source budget")
                                || error.contains("GPU selected slot budget")
                                || error.contains("GPU storage binding limit"))
                        {
                            stream.rejected_admission =
                                Some((stream.active.clone(), gpu_limit, source_capacity));
                        }
                        stream.submission_error = Some(error);
                        stream.trace_guard(
                            crate::diagnostics::sog_cache::ActivationGuard::SubmissionError,
                        );
                    }
                }
            }
        } else {
            // A camera may return to the last accepted selection after a failure.
            stream.submission_error = None;
            stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::PolicyWait);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod packed_tests;

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod camera_handoff_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
mod bounds_audit_tests;

/// Install progressive streaming over an already installed Gaussian renderer.
pub struct SplatStreamingPlugin;
impl Plugin for SplatStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SogLodSettings>()
            .init_resource::<crate::runtime::SogProjectionCache>()
            .init_resource::<crate::runtime::SogPagedSources>();
        install_sog_streaming(app);
        app.add_plugins(crate::diagnostics::quality::SplatDiagnosticsPlugin);
        #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
        {
            app.insert_resource(crate::renderer::SogRendererBackend::Snapshot);
            app.world_mut()
                .resource_mut::<crate::runtime::SogRuntimeSettings>()
                .backend = crate::renderer::SogRendererBackend::Snapshot;
        }
        app.add_systems(
            PostUpdate,
            update_sog_streams
                .after(TransformSystems::Propagate)
                .after(bevy::camera::CameraUpdateSystems)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
        );
    }
}
impl SogStream {
    /// Terminal preparation error or exhausted source retries; active coverage remains owned.
    pub fn terminal_error(&self) -> Option<&str> {
        self.submission_error.as_deref().or_else(|| {
            self.entries
                .iter()
                .any(|entry| !entry.can_retry() && entry.loading.is_none() && entry.ready.is_none())
                .then_some(self.last_error.as_deref())
                .flatten()
        })
    }
}

#[cfg(all(any(test, feature = "test-support"), not(target_arch = "wasm32")))]
#[doc(hidden)]
pub mod raster_handoff_tests;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;
