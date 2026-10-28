//! Bounded unbundled SOG transport. A file is decoded once and shared by leaves.

use crate::assets::FetchSlot;
use crate::assets::url_directory;
use crate::renderer::source::SourceStorage;
use bevy_gaussian_splatting::io::{sog, sog_packed};
use std::collections::HashMap;

pub(super) use crate::renderer::DecodedChunk;

type ChunkFiles = (sog::SogMeta, Vec<(String, FetchSlot)>);

#[allow(
    clippy::large_enum_variant,
    reason = "Load phases retain inline SOG metadata for one bounded request."
)]
enum Phase {
    Meta(FetchSlot<ChunkFiles>),
    Files(sog::SogMeta, Vec<(String, FetchSlot)>),
    #[cfg(target_arch = "wasm32")]
    Encoded(sog::SogMeta, HashMap<String, Vec<u8>>),
    #[cfg(target_arch = "wasm32")]
    Decoding(sog::SogMeta, super::codec::Job),
    Done,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tests/transport.rs"]
mod transport_tests;

pub(super) struct ChunkLoad {
    #[cfg(target_arch = "wasm32")]
    url: String,
    phase: Phase,
    #[cfg(target_arch = "wasm32")]
    max_splats: usize,
    started: f64,
    storage: SourceStorage,
    #[cfg(target_arch = "wasm32")]
    owner: u64,
}

impl ChunkLoad {
    /// Create a pending completion slot for deterministic acceptance tests.
    #[cfg(test)]
    pub(super) fn pending(now: f64) -> Self {
        Self {
            #[cfg(target_arch = "wasm32")]
            url: "pending/meta.json".into(),
            phase: Phase::Meta(FetchSlot::pending()),
            #[cfg(target_arch = "wasm32")]
            max_splats: 8,
            started: now,
            storage: SourceStorage::Expanded,
            #[cfg(target_arch = "wasm32")]
            owner: 0,
        }
    }

    /// Create a chunk load from captured metadata and texture bytes.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn captured(meta_bytes: &[u8], files: HashMap<String, Vec<u8>>, now: f64) -> Self {
        let meta = sog::parse_meta(meta_bytes).unwrap();
        #[cfg(target_arch = "wasm32")]
        let count = meta.count;
        let slots = meta
            .referenced_files()
            .into_iter()
            .map(|name| {
                (
                    name.to_string(),
                    FetchSlot::ready(
                        files
                            .get(name)
                            .cloned()
                            .ok_or_else(|| format!("missing fixture {name}")),
                    ),
                )
            })
            .collect();
        Self {
            #[cfg(target_arch = "wasm32")]
            url: "captured/meta.json".into(),
            phase: Phase::Files(meta, slots),
            #[cfg(target_arch = "wasm32")]
            max_splats: count,
            started: now,
            storage: SourceStorage::Expanded,
            #[cfg(target_arch = "wasm32")]
            owner: 0,
        }
    }

    /// Create the bounded state owned by this operation.
    #[cfg(test)]
    pub(super) fn new(
        url: String,
        expected: Option<usize>,
        max_splats: usize,
        now: f64,
        storage: SourceStorage,
    ) -> Self {
        Self::new_resolved(url, expected, max_splats, now, storage, Default::default())
    }

    /// Start metadata and texture requests through the supplied resolver.
    pub(super) fn new_resolved(
        url: String,
        expected: Option<usize>,
        max_splats: usize,
        now: f64,
        storage: SourceStorage,
        resolver: crate::assets::AssetResolver,
    ) -> Self {
        let directory = url_directory(&url).to_owned();
        let phase = Phase::Meta(FetchSlot::fetch_mapped_resolved(
            &url,
            1024 * 1024,
            &resolver.clone(),
            move |bytes| {
                let meta = sog::parse_meta(&bytes).map_err(|e| e.to_string())?;
                if meta.count > max_splats || expected.is_some_and(|n| n != meta.count) {
                    return Err("SOG chunk count exceeds budget or differs from hierarchy".into());
                }
                let names = match storage {
                SourceStorage::Expanded => meta.required_files_for_decode(bevy_gaussian_splatting::material::spherical_harmonics::SH_COEFF_COUNT_PER_CHANNEL),
                SourceStorage::Packed => meta.referenced_files(),
            };
                let slots = names
                    .into_iter()
                    .map(|name| {
                        (
                            name.to_string(),
                            resolver.fetch(&format!("{directory}{name}"), 32 * 1024 * 1024),
                        )
                    })
                    .collect();
                Ok((meta, slots))
            },
        ));
        Self {
            #[cfg(target_arch = "wasm32")]
            url,
            phase,
            #[cfg(target_arch = "wasm32")]
            max_splats,
            started: now,
            storage,
            #[cfg(target_arch = "wasm32")]
            owner: 0,
        }
    }

    /// Select the captured fixture source representation.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn with_storage(mut self, storage: SourceStorage) -> Self {
        self.storage = storage;
        self
    }

    /// Attach the scene identity used for cancellation and codec ownership.
    pub(super) fn with_owner(self, owner: u64) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            let mut load = self;
            load.owner = owner;
            load
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = owner;
            self
        }
    }

    /// Poll at most one completed decode per frame on the browser thread.
    pub(super) fn poll_measured(
        &mut self,
        now: f64,
        cpu: &crate::diagnostics::sog_cpu::SogCpuTimings,
    ) -> Option<Result<DecodedChunk, String>> {
        if now - self.started > 90.0 {
            return Some(Err("SOG chunk download timed out".into()));
        }
        match &self.phase {
            Phase::Meta(slot) => {
                let result = slot.take_result()?;
                match result {
                    Ok((meta, slots)) => {
                        self.phase = Phase::Files(meta, slots);
                        // Textures may already be complete by this app update.
                        self.poll_measured(now, cpu)
                    }
                    Err(error) => {
                        self.phase = Phase::Done;
                        Some(Err(error))
                    }
                }
            }
            Phase::Files(_, slots) if !slots.iter().all(|(_, s)| s.is_done()) => None,
            Phase::Files(_, _) => {
                let Phase::Files(meta, slots) = std::mem::replace(&mut self.phase, Phase::Done)
                else {
                    return Some(Err("SOG loader phase changed during completion".into()));
                };
                let files: Result<HashMap<_, _>, _> = slots
                    .into_iter()
                    .map(|(name, s)| {
                        s.take_result()
                            .ok_or_else(|| "SOG texture completion is unavailable".to_string())?
                            .map(|bytes| (name, bytes))
                    })
                    .collect();
                let files = match files {
                    Ok(files) => files,
                    Err(e) => return Some(Err(e)),
                };
                #[cfg(target_arch = "wasm32")]
                if super::codec::requested() && super::codec::available() {
                    self.phase = Phase::Encoded(meta, files);
                    return self.poll_measured(now, cpu);
                }
                Some(decode_sync(&meta, &files, self.storage, cpu))
            }
            #[cfg(target_arch = "wasm32")]
            Phase::Encoded(_, _) => {
                if super::codec::busy() {
                    return None;
                }
                let Phase::Encoded(meta, files) = std::mem::replace(&mut self.phase, Phase::Done)
                else {
                    return Some(Err("SOG codec phase changed".into()));
                };
                let job = cpu.measure(crate::diagnostics::sog_cpu::CpuStage::Decode, || {
                    super::codec::Job::start(
                        self.owner,
                        &self.url,
                        files,
                        packed_limits(meta.count).max_texture_bytes,
                        &meta,
                        self.storage == SourceStorage::Expanded,
                        false,
                        self.max_splats,
                        cpu.clone(),
                    )
                });
                match job {
                    Ok(job) => {
                        self.phase = Phase::Decoding(meta, job);
                        None
                    }
                    Err(e) => Some(Err(e)),
                }
            }
            #[cfg(target_arch = "wasm32")]
            Phase::Decoding(meta, job) => {
                let result = job.poll()?;
                let count = meta.count;
                let result = result.map(|decoded| {
                    let cloud = match decoded {
                        super::codec::Decoded::Expanded(cloud) => {
                            return DecodedChunk {
                                cloud: cloud.into(),
                                count,
                            };
                        }
                        super::codec::Decoded::Packed(cloud) => cloud,
                    };
                    DecodedChunk {
                        cloud: crate::renderer::source::SourceData::Packed(cloud.into()),
                        count,
                    }
                });
                self.phase = Phase::Done;
                Some(result)
            }
            Phase::Done => None,
        }
    }
}

fn packed_limits(count: usize) -> sog_packed::DecodeLimits {
    sog_packed::DecodeLimits {
        max_texture_dimension: 8192,
        max_texture_bytes: crate::renderer::source::packed_reservation(count).saturating_sub(8192)
            as u64,
    }
}

fn decode_sync(
    meta: &sog::SogMeta,
    files: &HashMap<String, Vec<u8>>,
    storage: SourceStorage,
    cpu: &crate::diagnostics::sog_cpu::SogCpuTimings,
) -> Result<DecodedChunk, String> {
    cpu.measure(
        crate::diagnostics::sog_cpu::CpuStage::Decode,
        || match storage {
            SourceStorage::Expanded => sog::decode_sog(meta, files).map(Into::into),
            SourceStorage::Packed => sog_packed::decode(meta, files, 3, packed_limits(meta.count))
                .map(|cloud| crate::renderer::source::SourceData::Packed(cloud.into())),
        },
    )
    .map(|cloud| DecodedChunk {
        cloud,
        count: meta.count,
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::sog_cpu::SogCpuTimings;

    #[test]
    fn decode_timing_excludes_transport_errors_and_counts_failed_decode_once() {
        let meta = br#"{
            "version":2,"count":1,
            "means":{"mins":[-1,-1,-1],"maxs":[1,1,1],"files":["lo.webp","hi.webp"]},
            "scales":{"codebook":[0],"files":["scales.webp"]},
            "quats":{"files":["quats.webp"]},
            "sh0":{"codebook":[0],"files":["sh0.webp"]}
        }"#;
        let cpu = SogCpuTimings::enabled();
        let mut transport_failure = ChunkLoad::captured(meta, HashMap::new(), 0.);
        assert!(
            transport_failure
                .poll_measured(0., &cpu)
                .expect("complete")
                .is_err()
        );
        assert_eq!(cpu.report().cpu_stages.expect("enabled").decode.attempts, 0);
        let files = [
            "lo.webp",
            "hi.webp",
            "scales.webp",
            "quats.webp",
            "sh0.webp",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), b"invalid WebP".to_vec()))
        .collect();
        let mut decode_failure = ChunkLoad::captured(meta, files, 0.);
        assert!(
            decode_failure
                .poll_measured(0., &cpu)
                .expect("complete")
                .is_err()
        );
        assert!(decode_failure.poll_measured(0., &cpu).is_none());
        let stages = cpu.report().cpu_stages.expect("enabled");
        assert_eq!(stages.decode.attempts, 1);
        assert!(stages.decode.total_ms.is_some());
    }
}
