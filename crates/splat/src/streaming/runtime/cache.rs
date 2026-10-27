//! Admission, retry and eviction policy for decoded source chunks.

use super::chunk::{ChunkLoad, DecodedChunk};

/// The decoder appends a tile even for an aligned logical input.
pub(super) fn reserved_rows(logical: usize) -> usize {
    logical.saturating_add(32 - logical % 32)
}

#[derive(Default)]
pub(super) struct Entry {
    pub ready: Option<DecodedChunk>,
    /// Evictable CPU copy of an uploaded whole-file source; never a GPU pin.
    pub warm: Option<DecodedChunk>,
    /// A real cache hit, not camera demand. Expires when the warm copy is trimmed.
    pub warm_reused: bool,
    pub loading: Option<ChunkLoad>,
    /// Immutable logical reservation made when a download starts (notably the
    /// environment's unknown count); later budget edits must not shrink it.
    pub reserved_splats: Option<usize>,
    pub attempts: u8,
    pub retry_at: f64,
    pub last_used: u64,
    pub ever_decoded: bool,
}

impl Entry {
    /// Report whether the retry allowance has not been exhausted.
    pub(super) fn can_retry(&self) -> bool {
        self.attempts < 3
    }

    /// Report whether this source may start a request at the supplied time.
    pub(super) fn eligible(&self, now: f64) -> bool {
        self.ready.is_none() && self.loading.is_none() && self.can_retry() && now >= self.retry_at
    }

    /// Publish a completed source or schedule bounded retry backoff.
    pub(super) fn complete(
        &mut self,
        result: Result<DecodedChunk, String>,
        now: f64,
    ) -> Option<String> {
        self.loading = None;
        self.reserved_splats = None;
        match result {
            Ok(chunk) => {
                self.ever_decoded = true;
                self.ready = Some(chunk);
                None
            }
            Err(error) => {
                self.retry_at = now + 2.0_f64.powi(i32::from(self.attempts));
                Some(error)
            }
        }
    }
}

/// Evict only obsolete, decoded files; in-flight and desired files are pinned.
/// Reservations include each decoder's alignment padding and environment cap.
#[cfg(test)]
pub(super) fn admit(
    entries: &mut [Entry],
    costs: &[usize],
    wanted: &[bool],
    incoming: usize,
    budget: usize,
) -> bool {
    admit_observed(entries, costs, wanted, incoming, budget, |_, _| {})
}

/// Admit the completed source against its observed allocation size.
pub(super) fn admit_observed(
    entries: &mut [Entry],
    costs: &[usize],
    wanted: &[bool],
    incoming: usize,
    budget: usize,
    mut evicted: impl FnMut(usize, u64),
) -> bool {
    // Plan admission before mutating the cache. Evicting some files and then
    // discovering that pinned/in-flight data prevents the download only creates
    // reload churn; it cannot make progress. Match GPU admission's preflight.
    let pinned = entries
        .iter()
        .zip(costs)
        .zip(wanted)
        .filter(|((entry, _), wanted)| {
            entry.loading.is_some() || (**wanted && entry.ready.is_some())
        })
        .fold(0usize, |bytes, ((_, cost), _)| bytes.saturating_add(*cost));
    if pinned.saturating_add(costs[incoming]) > budget {
        return false;
    }
    let mut used: usize = entries
        .iter()
        .zip(costs)
        .filter(|(e, _)| e.ready.is_some() || e.loading.is_some())
        .fold(0usize, |used, (_, n)| used.saturating_add(*n));
    while used.saturating_add(costs[incoming]) > budget {
        let oldest = entries
            .iter()
            .enumerate()
            .filter(|(i, e)| !wanted[*i] && e.ready.is_some() && e.loading.is_none())
            .min_by_key(|(_, e)| e.last_used)
            .map(|(i, _)| i);
        let Some(i) = oldest else {
            return false;
        };
        if let Some(chunk) = entries[i].ready.take() {
            evicted(i, chunk.cloud.retained_bytes());
        }
        entries[i].attempts = 0;
        used -= costs[i];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_gaussian_splatting::PlanarGaussian3d;
    fn resident(last_used: u64) -> Entry {
        Entry {
            ready: Some(DecodedChunk {
                cloud: PlanarGaussian3d::default().into(),
                count: 2,
            }),
            last_used,
            ..Default::default()
        }
    }

    #[test]
    fn evicts_old_unused_chunks_but_never_active_coverage() {
        let mut e = vec![resident(0), resident(1), resident(2), Entry::default()];
        let mut evictions = Vec::new();
        assert!(admit_observed(
            &mut e,
            &[2, 2, 2, 3],
            &[true, false, false, true],
            3,
            7,
            |file, bytes| evictions.push((file, bytes)),
        ));
        assert_eq!(evictions.len(), 1);
        assert_eq!(
            evictions[0].0, 1,
            "trace must identify the evicted file, not the incoming one"
        );
        assert!(e[0].ready.is_some());
        assert!(e[1].ready.is_none());
        assert!(e[2].ready.is_some());
        assert!(!admit(&mut e, &[2, 2, 2, 3], &[true; 4], 3, 5));
    }

    #[test]
    fn rejected_admission_keeps_useful_cache_entries_intact() {
        let mut entries = vec![resident(0), resident(1), Entry::default()];
        let mut evictions = Vec::new();
        // Evicting file 0 still cannot fit the incoming file beside pinned 1.
        assert!(!admit_observed(
            &mut entries,
            &[3, 8, 5],
            &[false, true, true],
            2,
            12,
            |file, _| evictions.push(file)
        ));
        assert!(
            entries[0].ready.is_some(),
            "failed admission must not evict useful data"
        );
        assert!(evictions.is_empty());
    }

    #[test]
    fn inflight_reservations_and_old_generation_pin_block_admission() {
        let mut entries = vec![
            resident(0),
            Entry {
                loading: Some(ChunkLoad::captured(
                    br#"{
                    "version":2,"count":8,
                    "means":{"mins":[-1,-1,-1],"maxs":[1,1,1],"files":["lo.webp","hi.webp"]},
                    "scales":{"codebook":[0],"files":["scales.webp"]},
                    "quats":{"files":["quats.webp"]},
                    "sh0":{"codebook":[0],"files":["sh0.webp"]}
                }"#,
                    Default::default(),
                    0.,
                )),
                ..Default::default()
            },
            Entry::default(),
        ];
        assert!(!admit(&mut entries, &[8, 8, 4], &[true, true, true], 2, 19));
        assert!(entries[0].ready.is_some());
        assert!(entries[1].loading.is_some());
        assert!(admit(&mut entries, &[8, 8, 4], &[true, true, true], 2, 20));
    }

    #[test]
    fn retries_are_delayed_bounded_and_do_not_duplicate_resident_downloads() {
        let mut e = Entry::default();
        for attempt in 1..=3 {
            e.attempts = attempt;
            e.complete(Err("network".into()), 10.0);
            assert!(!e.eligible(10.1));
            assert_eq!(e.eligible(100.0), attempt < 3);
        }
        let e = resident(0);
        assert!(!e.eligible(100.0));
    }
}
