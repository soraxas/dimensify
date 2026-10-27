//! Reuse validated packed texels after GPU eviction, within the decoded cap.
//! Warm copies are expendable before any new immutable download reservation.

use super::{cache::Entry, chunk::DecodedChunk};
use crate::renderer::source::{SourceData, packed_reservation};
use std::cmp::Reverse;

/// Publish completed warm sources into the admitted cache.
pub(super) fn sync(
    entry: &mut Entry,
    synchronize: impl FnOnce(&mut Option<DecodedChunk>) -> bool,
) -> bool {
    // PackedSog clones share immutable texture allocations. The renderer's
    // readiness mirror intentionally owns no GPU allocation and cannot pin it.
    let staging = entry.ready.as_ref().and_then(|chunk| match &chunk.cloud {
        SourceData::Packed(data) => Some(DecodedChunk {
            count: chunk.count,
            cloud: SourceData::Packed(data.clone()),
        }),
        _ => None,
    });
    let invalidated = synchronize(&mut entry.ready);
    if entry
        .ready
        .as_ref()
        .is_some_and(|chunk| matches!(chunk.cloud, SourceData::ResidentPacked(_)))
        && staging.is_some()
    {
        entry.warm = staging;
    }
    invalidated
}

/// Call only after ordinary source admission, for a currently requested file.
/// GPU eviction alone must not convert expendable cache into live staging.
pub(super) fn restore(entry: &mut Entry) -> bool {
    if entry.ready.is_some() || entry.loading.is_some() || entry.warm.is_none() {
        return false;
    }
    entry.ready = entry.warm.take();
    entry.warm_reused = true;
    true
}

/// Reclaim unpinned warm sources within the cache ceiling.
pub(super) fn trim(
    entries: &mut [Entry],
    capacity: usize,
    incoming: usize,
    mut evicted: impl FnMut(usize, u64),
) {
    let mut used = entries.iter().fold(incoming as u64, |sum, entry| {
        let data = entry
            .ready
            .iter()
            .chain(&entry.warm)
            .fold(0u64, |sum, chunk| {
                sum.saturating_add(chunk.cloud.retained_bytes())
            });
        sum.saturating_add(data).saturating_add(
            entry
                .reserved_splats
                .map_or(0, |n| packed_reservation(n) as u64),
        )
    });
    while used > capacity as u64 {
        let Some(file) = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.warm.is_some())
            .min_by_key(|(file, entry)| {
                let gpu_backed = entry
                    .ready
                    .as_ref()
                    .is_some_and(|chunk| matches!(chunk.cloud, SourceData::ResidentPacked(_)));
                // CPU and GPU must not evict the same cold source in lockstep.
                // Recent GPU use already protects a source there: discard its
                // redundant CPU copy before likely GPU victims. Once GPU has
                // evicted a source, CPU is its only cache: retain it ahead of
                // redundant copies and use ordinary LRU within that tier.
                (
                    !gpu_backed,
                    // Among redundant GPU-backed copies, keep demonstrated
                    // return-path hits ahead of first-use backups. This is
                    // not a pin; CPU-only history still uses ordinary LRU.
                    gpu_backed && entry.warm_reused,
                    if gpu_backed {
                        Reverse(entry.last_used)
                    } else {
                        Reverse(0)
                    },
                    if gpu_backed { 0 } else { entry.last_used },
                    *file,
                )
            })
            .map(|(file, _)| file)
        else {
            break;
        };
        let chunk = entries[file].warm.take().expect("selected warm owner");
        entries[file].warm_reused = false;
        let bytes = chunk.cloud.retained_bytes();
        used = used.saturating_sub(bytes);
        evicted(file, bytes);
        // Live staging and immutable in-flight reservations may exceed a newly
        // lowered cap. Never cancel them or invent GPU readiness to make it fit.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::source::{PackedSourceDescriptor, SourceData, packed_reservation};
    use bevy_gaussian_splatting::io::sog_packed::{DataTexture, DecodeLimits, decode_rgba};

    fn chunk() -> DecodedChunk {
        let mut fixture: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/worker-expanded-v2.json")).unwrap();
        fixture["meta"].as_object_mut().unwrap().remove("shN");
        let meta = serde_json::from_value(fixture["meta"].clone()).unwrap();
        let textures = [
            "lo.webp",
            "hi.webp",
            "quats.webp",
            "scales.webp",
            "sh0.webp",
        ]
        .into_iter()
        .map(|name| {
            let mut bytes = vec![0; 2 * 4];
            if name == "quats.webp" {
                for pixel in bytes.chunks_exact_mut(4) {
                    pixel[3] = 252;
                }
            }
            (
                name.to_string(),
                DataTexture::from_rgba(2, 1, bytes).unwrap(),
            )
        })
        .collect();
        DecodedChunk {
            cloud: SourceData::Packed(
                decode_rgba(
                    &meta,
                    &textures,
                    0,
                    DecodeLimits {
                        max_texture_dimension: 4096,
                        max_texture_bytes: 1024,
                    },
                )
                .unwrap()
                .into(),
            ),
            count: 2,
        }
    }

    fn mirror() -> DecodedChunk {
        DecodedChunk {
            count: 2,
            cloud: SourceData::ResidentPacked(PackedSourceDescriptor {
                count: 2,
                group: (false, 0),
                gpu_bytes: 8192,
            }),
        }
    }

    fn uploaded(last_used: u64) -> Entry {
        Entry {
            ready: Some(mirror()),
            warm: Some(chunk()),
            last_used,
            ..Default::default()
        }
    }

    #[test]
    fn gpu_eviction_reuses_the_same_validated_texels_without_another_decode() {
        let mut entry = Entry {
            ready: Some(chunk()),
            ..Default::default()
        };
        let ptr = entry
            .ready
            .as_ref()
            .unwrap()
            .cloud
            .packed()
            .unwrap()
            .means_lo()
            .texels()
            .as_ptr();
        assert_eq!(
            sync(&mut entry, |ready| {
                *ready = Some(mirror());
                false
            }),
            false
        );
        assert!(
            entry.warm.is_some(),
            "GPU ownership must not force discard of useful CPU data"
        );
        assert_eq!(
            sync(&mut entry, |ready| {
                *ready = None;
                true
            }),
            true
        );
        assert!(entry.ready.is_none(), "only a requested file is restored");
        assert!(restore(&mut entry));
        assert!(
            entry.warm.is_none(),
            "move ownership instead of double charging"
        );
        assert_eq!(
            entry
                .ready
                .as_ref()
                .unwrap()
                .cloud
                .packed()
                .unwrap()
                .means_lo()
                .texels()
                .as_ptr(),
            ptr
        );
        assert!(
            !entry.eligible(100.),
            "a warm hit must not start a network request"
        );
    }

    #[test]
    fn pending_upload_and_expanded_sources_do_not_make_extra_warm_owners() {
        let mut entry = Entry {
            ready: Some(chunk()),
            ..Default::default()
        };
        assert!(!sync(&mut entry, |_| false));
        assert!(entry.warm.is_none());
        entry.ready = Some(DecodedChunk {
            cloud: Default::default(),
            count: 0,
        });
        sync(&mut entry, |_| false);
        assert!(entry.warm.is_none());
    }

    #[test]
    fn trim_charges_live_data_and_download_reservations_before_warm_data() {
        let mut entries = vec![
            uploaded(8),
            uploaded(1),
            Entry {
                reserved_splats: Some(1024),
                ..Default::default()
            },
        ];
        let warm = entries[0].warm.as_ref().unwrap().cloud.retained_bytes() as usize;
        let mirrors = 2 * mirror().cloud.retained_bytes() as usize;
        let capacity = mirrors + packed_reservation(1024) + warm;
        let mut evictions = Vec::new();
        trim(&mut entries, capacity, 0, |i, bytes| {
            evictions.push((i, bytes))
        });
        assert_eq!(
            evictions,
            [(0, warm as u64)],
            "recent GPU use makes this CPU copy redundant"
        );
        assert!(entries[1].warm.is_some());
        assert!(
            entries[0].ready.is_some(),
            "dropping CPU cache must not invalidate GPU readiness"
        );
        assert_eq!(entries[2].reserved_splats, Some(1024));
        trim(&mut entries, capacity, warm, |_, _| {});
        assert!(
            entries.iter().all(|e| e.warm.is_none()),
            "new reservation gets space before starting"
        );
        assert!(entries[0].ready.is_some());
    }

    #[test]
    fn gpu_backed_copies_yield_to_evicted_sources_and_keep_next_victims() {
        let mut entries = vec![uploaded(1), uploaded(8), uploaded(0)];
        entries[2].ready = None; // Already evicted from GPU, potentially needed on return.
        let warm = entries[0].warm.as_ref().unwrap().cloud.retained_bytes() as usize;
        let mirrors = 2 * mirror().cloud.retained_bytes() as usize;
        trim(&mut entries, mirrors + 2 * warm, 0, |_, _| {});
        assert!(
            entries[1].warm.is_none(),
            "recent GPU use already protects this copy"
        );
        assert!(
            entries[0].warm.is_some(),
            "retain the likely next GPU victim"
        );
        assert!(
            entries[2].warm.is_some(),
            "only CPU can serve this source now"
        );
        trim(&mut entries, mirrors + warm, 0, |_, _| {});
        assert!(entries[0].warm.is_none());
        assert!(entries[0].ready.is_some());
        assert!(entries[2].warm.is_some());
    }

    #[test]
    fn turn_back_recovers_cold_gpu_victim_without_redownload() {
        // Browser source-4 regression: protecting only current demand discarded
        // its CPU copy just before GPU pressure removed the other copy. Keep
        // the next likely GPU victim recoverable, not just the current view.
        let mut entries = vec![uploaded(8), uploaded(17), uploaded(12)];
        entries[2].ready = None;
        let warm = chunk().cloud.retained_bytes() as usize;
        let mirrors = 2 * mirror().cloud.retained_bytes() as usize;
        let incoming = packed_reservation(2);
        let capacity = mirrors + 2 * warm + incoming;
        trim(&mut entries, capacity, incoming, |_, _| {});
        assert!(entries[0].warm.is_some(), "cold GPU victim keeps a backup");
        assert!(entries[1].warm.is_none(), "current GPU copy remains usable");
        assert!(entries[1].ready.is_some());
        assert!(
            entries[2].warm.is_some(),
            "CPU-only history remains recoverable"
        );

        // GPU admission removes the old view; the reverse orbit needs it again.
        sync(&mut entries[0], |ready| {
            *ready = None;
            true
        });
        assert!(restore(&mut entries[0]));
        assert!(
            !entries[0].eligible(100.),
            "return must not issue a download"
        );
        assert!(entries[0].warm.is_none(), "restoration transfers ownership");
    }

    #[test]
    fn repeated_return_keeps_reused_backup_before_first_use_duplicates() {
        let mut entries = vec![uploaded(40), uploaded(39), uploaded(1)];
        entries[2].ready = None; // Sole CPU copy still precedes redundant copies.
        sync(&mut entries[0], |ready| {
            *ready = None;
            true
        });
        assert!(restore(&mut entries[0]));
        sync(&mut entries[0], |ready| {
            *ready = Some(mirror());
            false
        });
        let warm = chunk().cloud.retained_bytes() as usize;
        let mirrors = 2 * mirror().cloud.retained_bytes() as usize;
        trim(&mut entries, mirrors + 2 * warm, 0, |_, _| {});
        assert!(entries[1].warm.is_none(), "first-use GPU duplicate yields");
        assert!(
            entries[2].warm.is_some(),
            "do not sacrifice CPU-only history"
        );
        sync(&mut entries[0], |ready| {
            *ready = None;
            true
        });
        assert!(
            restore(&mut entries[0]),
            "repeated return stays a local hit"
        );
        assert!(!entries[0].eligible(100.));
    }

    #[test]
    fn reuse_preference_yields_to_cap_and_expires_with_the_copy() {
        let mut entries = vec![uploaded(8), uploaded(7)];
        sync(&mut entries[0], |ready| {
            *ready = None;
            true
        });
        assert!(restore(&mut entries[0]));
        sync(&mut entries[0], |ready| {
            *ready = Some(mirror());
            false
        });
        trim(&mut entries, 0, 0, |_, _| {});
        assert!(entries.iter().all(|entry| entry.warm.is_none()));
        assert!(entries.iter().all(|entry| entry.ready.is_some()));

        // A subsequent cold load is not permanently promoted by an old hit.
        entries[0].ready = Some(chunk());
        sync(&mut entries[0], |ready| {
            *ready = Some(mirror());
            false
        });
        entries[1].warm = Some(chunk());
        let capacity =
            2 * mirror().cloud.retained_bytes() as usize + chunk().cloud.retained_bytes() as usize;
        trim(&mut entries, capacity, 0, |_, _| {});
        assert!(
            entries[0].warm.is_none(),
            "expired preference must not persist"
        );
        assert!(entries[1].warm.is_some());
    }

    #[test]
    fn gpu_evicted_warm_copies_use_lru_among_themselves() {
        let mut entries = vec![uploaded(8), uploaded(1)];
        for entry in &mut entries {
            entry.ready = None;
        }
        let warm = entries[0].warm.as_ref().unwrap().cloud.retained_bytes() as usize;
        trim(&mut entries, warm, 0, |_, _| {});
        assert!(entries[0].warm.is_some());
        assert!(entries[1].warm.is_none());
    }

    #[test]
    fn requested_warm_file_bypasses_cold_requests_with_full_network_slots() {
        use super::super::{SogLodSettings, SogStream, chunk::ChunkLoad};
        use crate::renderer::source::SourceStorage;
        use bevy::prelude::Vec3;
        let leaf = |i| {
            serde_json::json!({
                "bound": {"min": [i * 2,-1,-1], "max": [i * 2 + 1,1,1]},
                "lods": {"0": {"file": i, "offset": 0, "count": 2}}
            })
        };
        let manifest = serde_json::json!({
            "lodLevels": 1,
            "filenames": ["first/meta.json", "second/meta.json", "warm/meta.json"],
            "tree": {
                "bound": {"min": [-1,-1,-1], "max": [6,1,1]},
                "children": [leaf(0), {
                    "bound": {"min": [2,-1,-1], "max": [6,1,1]},
                    "children": [leaf(1), leaf(2)]
                }]
            }
        });
        let mut stream = SogStream::parse(
            &serde_json::to_vec(&manifest).unwrap(),
            "https://example.test/lod-meta.json",
            "warm-test".into(),
            "warm-test".into(),
        )
        .unwrap();
        stream.storage = SourceStorage::Packed;
        stream.entries[2].warm = Some(chunk());
        let ptr = stream.entries[2]
            .warm
            .as_ref()
            .unwrap()
            .cloud
            .packed()
            .unwrap()
            .means_lo()
            .texels()
            .as_ptr();
        let settings = SogLodSettings {
            concurrent_chunks: 1,
            cache_bytes: 3 * packed_reservation(2),
            ..Default::default()
        };
        let mut requests = Vec::new();
        stream.tick(
            Vec3::ZERO.into(),
            0.,
            &settings,
            100,
            None,
            |url, _, _, now| {
                requests.push(url);
                let fixture: serde_json::Value =
                    serde_json::from_str(include_str!("fixtures/worker-expanded-v2.json")).unwrap();
                ChunkLoad::captured(
                    &serde_json::to_vec(&fixture["meta"]).unwrap(),
                    Default::default(),
                    now,
                )
            },
        );
        assert_eq!(requests, ["https://example.test/first/meta.json"]);
        assert!(stream.entries[1].loading.is_none());
        assert!(stream.entries[2].warm.is_none());
        assert_eq!(
            stream.entries[2]
                .ready
                .as_ref()
                .unwrap()
                .cloud
                .packed()
                .unwrap()
                .means_lo()
                .texels()
                .as_ptr(),
            ptr
        );
        let (ready, reserved, _) = stream.cache_memory();
        assert!(ready + reserved <= settings.cache_bytes as u64);
        assert!(stream.active.iter().any(|range| range.file == 2));
    }

    #[test]
    fn gpu_eviction_then_cap_shrink_keeps_unused_texels_expendable() {
        let mut entries = vec![uploaded(1)];
        sync(&mut entries[0], |ready| {
            *ready = None;
            true
        });
        trim(&mut entries, 0, 0, |_, _| {});
        assert!(entries[0].ready.is_none());
        assert!(entries[0].warm.is_none());
    }

    #[test]
    fn missing_mirror_does_not_make_cached_texels_unreachable() {
        let mut entry = uploaded(1);
        entry.ready = None;
        sync(&mut entry, |_| false);
        // A requested warm file must be recoverable even if CPU admission
        // removed its non-owning mirror before GPU eviction was observed.
        assert!(restore(&mut entry), "cached data must not be ignored");
        assert!(entry.ready.is_some());
    }

    #[test]
    fn lowered_cap_drops_warm_copies_without_canceling_live_work() {
        let mut entries = vec![uploaded(1), uploaded(2)];
        trim(&mut entries, 0, 0, |_, _| {});
        assert!(entries.iter().all(|e| e.warm.is_none()));
        assert!(entries.iter().all(|e| e.ready.is_some()));
        assert_eq!(
            sync(&mut entries[0], |ready| {
                *ready = None;
                true
            }),
            true
        );
        assert!(
            entries[0].eligible(100.),
            "a real warm miss still permits reloading"
        );
    }
}
