use super::*;
use bevy_gaussian_splatting::io::sog_packed::{DataTexture, decode_rgba};

fn source(count: usize) -> PackedSog {
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/worker-expanded-v2.json")).unwrap();
    json["meta"]["count"] = count.into();
    let meta = serde_json::from_value(json["meta"].clone()).unwrap();
    let height = count.div_ceil(256);
    let textures = [
        "lo.webp",
        "hi.webp",
        "quats.webp",
        "scales.webp",
        "sh0.webp",
        "labels.webp",
        "centroids.webp",
    ]
    .into_iter()
    .map(|name| {
        let (w, h) = if name == "centroids.webp" {
            (960, 1)
        } else {
            (256, height)
        };
        let mut bytes = vec![0; w * h * 4];
        if name == "quats.webp" {
            for px in bytes.as_chunks_mut::<4>().0 {
                px[3] = 252;
            }
        }
        (
            name.to_string(),
            DataTexture::from_rgba(w as u32, h as u32, bytes).unwrap(),
        )
    })
    .collect();
    decode_rgba(
        &meta,
        &textures,
        3,
        DecodeLimits {
            max_texture_dimension: 8192,
            max_texture_bytes: 64 << 20,
        },
    )
    .unwrap()
}

#[test]
fn page_cache_preserves_range_order_and_boundary_offsets() {
    let source = source(PAGE * 2 + 8);
    let mut cache = PageCache::new(vec![source.count() as usize]).unwrap();
    cache.observe(0, &source).unwrap();
    let range = SplatRange {
        file: 0,
        offset: PAGE - 2,
        count: PAGE + 6,
    };
    assert!(!cache.covers(range));
    for page in 0..3 {
        cache
            .insert(Key { file: 0, page }, &source, 64 << 20, 1)
            .unwrap();
    }
    assert!(cache.covers(range));
    let split = cache.split(&[range]).unwrap();
    assert_eq!(
        split
            .iter()
            .map(|r| (r.offset, r.count))
            .collect::<Vec<_>>(),
        [(PAGE - 2, 2), (0, PAGE), (0, 4)]
    );
    assert_eq!(split.iter().map(|r| r.count).sum::<usize>(), range.count);
    for r in split {
        assert!(r.offset + r.count <= cache.source(r.file).unwrap().count);
    }
}

#[test]
fn page_cache_shares_palette_on_refetch_and_evicts_only_unpinned_pages() {
    let source = source(PAGE * 2);
    let mut cache = PageCache::new(vec![source.count() as usize]).unwrap();
    cache.observe(0, &source).unwrap();
    let a = Key { file: 0, page: 0 };
    let b = Key { file: 0, page: 1 };
    cache.insert(a, &source, 64 << 20, 1).unwrap();
    let first = cache.resident_bytes();
    let needed = cache.cost(&[a, b].into_iter().collect());
    assert!(cache.insert(b, &source, needed - 1, 2).is_err());
    assert_eq!(
        cache.resident_bytes(),
        first,
        "failed admission cannot mutate residency"
    );
    let mut refetched = source.clone();
    cache.reuse_palette(0, &mut refetched).unwrap();
    cache.insert(b, &refetched, needed, 2).unwrap();
    assert_eq!(cache.resident_bytes(), needed);
    assert!(needed < first * 2, "palette charged once");
    assert!(!cache.make_room(&[a, b].into_iter().collect(), 0, first));
    assert!(cache.covers(SplatRange {
        file: 0,
        offset: 0,
        count: PAGE * 2
    }));
    assert!(cache.make_room(&[b].into_iter().collect(), 0, first));
    assert!(!cache.covers(SplatRange {
        file: 0,
        offset: 0,
        count: 1
    }));
    assert!(cache.covers(SplatRange {
        file: 0,
        offset: PAGE,
        count: 1
    }));
    assert_eq!(cache.resident_bytes(), first);
}

#[test]
fn page_admission_reserves_not_yet_copied_destination_before_eviction() {
    let source = source(PAGE * 2);
    let mut cache = PageCache::new(vec![PAGE * 2]).unwrap();
    cache.observe(0, &source).unwrap();
    let a = Key { file: 0, page: 0 };
    let b = Key { file: 0, page: 1 };
    cache.insert(a, &source, 64 << 20, 1).unwrap();
    let capacity = cache.resident_bytes();
    assert!(cache.make_room(&[b].into_iter().collect(), 0, capacity));
    assert!(
        !cache.pages.contains_key(&a),
        "future page must reserve room before copy"
    );
    cache.insert(b, &source, capacity, 2).unwrap();
    assert_eq!(cache.resident_bytes(), capacity);
}

#[test]
fn page_copy_progress_does_not_wait_for_or_mutate_renderer_handoff() {
    use super::super::*;
    let count = PAGE * 3;
    let manifest = serde_json::json!({"lodLevels":2,"filenames":["coarse/meta.json","fine/meta.json"],
        "tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{
            "0":{"file":1,"offset":0,"count":count},"1":{"file":0,"offset":0,"count":1}}}});
    let mut stream = SogStream::parse(
        &serde_json::to_vec(&manifest).unwrap(),
        "https://fixture/lod-meta.json",
        "test".into(),
        "test".into(),
    )
    .unwrap();
    stream.storage = SourceStorage::Packed;
    stream.enable_pages().unwrap();
    let coarse = source(1);
    let fine = source(count);
    let range = SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    };
    let cache = stream.pages.as_mut().unwrap();
    cache.observe(0, &coarse).unwrap();
    cache
        .insert(Key { file: 0, page: 0 }, &coarse, 64 << 20, 0)
        .unwrap();
    cache.pending = cache
        .keys(SplatRange {
            file: 1,
            offset: 0,
            count,
        })
        .unwrap();
    stream.entries[1].ready = Some(DecodedChunk {
        count,
        cloud: SourceData::Packed(fine.into()),
    });
    stream.active = vec![range];
    stream.submitted = vec![range];
    stream.displayed = vec![range];
    stream.preparing_upload = true;
    for expected in 1..=3 {
        stream.next_update = 100.;
        stream.progress_pages(64 << 20);
        assert_eq!(
            stream.pages.as_ref().unwrap().pages.len(),
            1 + expected,
            "one page per update"
        );
        assert_eq!(stream.active, [range]);
        assert_eq!(stream.submitted, [range]);
        assert_eq!(stream.displayed, [range]);
        assert!(
            stream.preparing_upload,
            "copy progress cannot acknowledge a renderer upload"
        );
        assert_eq!(
            stream.next_update, 0.,
            "ready pages must be evaluated promptly"
        );
        assert!(
            stream.entries[1].ready.is_some(),
            "spare capacity should preserve the recent decode"
        );
    }
    assert!(!stream.preparing_pages);
    assert_eq!(
        stream.generation, 0,
        "copying must not run the camera policy"
    );
    let page_bytes = stream.pages.as_ref().unwrap().resident_bytes() as usize;
    stream.progress_pages(page_bytes);
    assert!(
        stream.entries[1].ready.is_none(),
        "decoded reuse must yield to the page budget"
    );
    assert_eq!(
        stream.pages.as_ref().unwrap().pages.len(),
        4,
        "completed staging is evicted before pages"
    );
    assert_eq!(stream.status().cache_ready_bytes, page_bytes as u64);
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR; real decoder, no renderer readiness/GPU claims"]
fn paged_kitchen_replay_reaches_detail_and_returns_under_the_same_cap() {
    use super::super::*;
    let root = std::path::PathBuf::from(std::env::var_os("SOG_REPLAY_DIR").unwrap());
    let mut stream = SogStream::parse(
        &std::fs::read(root.join("lod-meta.json")).unwrap(),
        "https://capture/lod-meta.json",
        "pages".into(),
        "pages".into(),
    )
    .unwrap();
    stream.storage = SourceStorage::Packed;
    stream.enable_pages().unwrap();
    let settings = SogLodSettings {
        cache_bytes: 256 << 20,
        splat_budget: 4_000_000,
        ..default()
    };
    for (phase, eye) in [
        Vec3::new(-2.01, 0.127, -6.347),
        Vec3::new(8., 1., -35.),
        Vec3::new(-2.01, 0.127, -6.347),
    ]
    .into_iter()
    .enumerate()
    {
        let mut settled = false;
        let mut requests = 0;
        for tick in 0..2000 {
            stream.tick(
                eye.into(),
                (phase * 2000 + tick) as f64,
                &settings,
                4_000_000,
                Some(256 << 20),
                |url, _, _, now| {
                    requests += 1;
                    let path = root.join(url.strip_prefix("https://capture/").unwrap());
                    let bytes = std::fs::read(&path).unwrap();
                    let meta = bevy_gaussian_splatting::io::sog::parse_meta(&bytes).unwrap();
                    let files = meta
                        .referenced_files()
                        .into_iter()
                        .map(|name| {
                            (
                                name.to_string(),
                                std::fs::read(path.parent().unwrap().join(name)).unwrap(),
                            )
                        })
                        .collect();
                    ChunkLoad::captured(&bytes, files, now).with_storage(SourceStorage::Packed)
                },
            );
            let cache = stream.pages.as_ref().unwrap();
            let mapped = cache.split(&stream.active).unwrap();
            assert_eq!(
                mapped.iter().map(|r| r.count).sum::<usize>(),
                stream.active.iter().map(|r| r.count).sum::<usize>()
            );
            for r in mapped {
                assert!(r.offset + r.count <= cache.source(r.file).unwrap().count);
            }
            let status = stream.status();
            assert!(
                status.cache_ready_bytes + status.cache_reserved_bytes
                    <= settings.cache_bytes as u64,
                "phase {phase} tick {tick}: decoded/reserved cap"
            );
            stream.submitted = stream.active.clone();
            stream.displayed = stream.active.clone();
            if !stream.pending_work {
                settled = true;
                break;
            }
        }
        assert!(
            settled,
            "phase {phase} did not settle: {:?}",
            stream.last_error
        );
        let count = stream.status().displayed;
        let cache = stream.pages.as_ref().unwrap();
        let ideal = selection::target_ranges(
            &stream.manifest,
            eye.into(),
            4_000_000 - stream.environment_range().map_or(0, |r| r.count),
        );
        let keys = cache
            .union(
                ideal
                    .iter()
                    .flatten()
                    .copied()
                    .chain(
                        stream
                            .manifest
                            .leaves
                            .iter()
                            .map(|l| l.lods.last().unwrap().1),
                    )
                    .chain(stream.environment_range()),
            )
            .unwrap();
        eprintln!(
            "paged phase {phase}: {count} displayed-model splats, {requests} files, {} cache bytes; ideal pages {} bytes, {} files, capped {}",
            stream.status().cache_ready_bytes,
            cache.cost(&keys),
            keys.iter().map(|k| k.file).collect::<BTreeSet<_>>().len(),
            stream.admission.source_limited
        );
        assert_eq!(
            count,
            ideal.iter().flatten().map(|r| r.count).sum::<usize>()
                + stream.environment_range().map_or(0, |r| r.count),
            "all budgeted target ranges must become resident"
        );
        if phase != 1 {
            assert!(
                count >= 3_750_000,
                "the reference and return poses approach the 4M selected target"
            );
        }
        assert!(stream.last_error.is_none(), "{:?}", stream.last_error);
        if std::env::var_os("SOG_REPLAY_PROFILE").is_some() {
            let previous = stream.active.clone();
            stream.cpu_timings = crate::diagnostics::sog_cpu::SogCpuTimings::enabled();
            for i in 0..200 {
                stream.tick(
                    eye.into(),
                    (phase * 2000 + 1800 + i) as f64,
                    &settings,
                    4_000_000,
                    Some(256 << 20),
                    |_, _, _, _| panic!("settled camera must reuse resident pages"),
                );
                assert_eq!(stream.active, previous);
            }
            eprintln!(
                "settled page phase {phase}: {}",
                serde_json::to_string(&stream.cpu_timings.report()).unwrap()
            );
        }
    }
}
