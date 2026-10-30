//! Scene-owned stream construction, status/memory reporting and transport polling.
use super::*;

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

    pub(super) fn environment_index(&self) -> Option<usize> {
        self.manifest
            .environment
            .as_ref()
            .map(|_| self.manifest.filenames.len())
    }

    /// Only decoded, nonempty environment data contributes renderable coverage.
    pub(super) fn environment_range(&self) -> Option<SplatRange> {
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

    /// Reconcile renderer residency without giving cache entries ownership of
    /// the currently displayed scene. The callback owns GPU truth.
    pub(super) fn sync_resident_sources(
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
    pub(super) fn poll_loads(&mut self, now: f64) {
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
