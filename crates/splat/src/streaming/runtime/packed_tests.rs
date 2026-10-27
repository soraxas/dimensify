//! Filesystem-only packed admission evidence; does not claim raster/GPU residency.
use super::*;

fn load(root: &std::path::Path, path: &str, now: f64) -> ChunkLoad {
    let meta_path = root.join(path);
    let bytes = std::fs::read(&meta_path).expect(path);
    let meta = bevy_gaussian_splatting::io::sog::parse_meta(&bytes).expect("metadata");
    let files = meta
        .referenced_files()
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                std::fs::read(meta_path.parent().expect("directory").join(name)).expect(name),
            )
        })
        .collect();
    ChunkLoad::captured(&bytes, files, now).with_storage(SourceStorage::Packed)
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR; filesystem-only packed source validation"]
fn all_saved_sources_fit_packed_reservations_with_full_sh() {
    let root =
        std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").expect("fixture directory"));
    let manifest =
        Manifest::parse(&std::fs::read(root.join("lod-meta.json")).expect("manifest")).unwrap();
    let mut bytes = 0u64;
    let mut logical = 0usize;
    for path in manifest.filenames.iter().chain(manifest.environment.iter()) {
        let source = load(&root, path, 0.)
            .poll_measured(0., &SogCpuTimings::default())
            .expect("complete")
            .expect(path);
        let packed = source.cloud.packed().unwrap();
        assert!(
            source.cloud.admission_bytes() <= packed_reservation(source.count) as u64,
            "{path}"
        );
        assert!(
            packed
                .higher_order()
                .is_none_or(|sh| sh.bands() == sh.encoded_bands()),
            "all encoded bands: {path}"
        );
        eprintln!(
            "{path}: {} splats, {} packed resident bytes",
            source.count,
            source.cloud.admission_bytes()
        );
        bytes += source.cloud.admission_bytes();
        logical += source.count;
    }
    eprintln!(
        "all sources: {logical} splats; {bytes} packed resident bytes (not simultaneous residency)"
    );
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR; filesystem-only packed scheduler replay"]
fn packed_kitchen_refines_and_relocates_under_unchanged_256_mib_cap() {
    let root =
        std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").expect("fixture directory"));
    let mut stream = SogStream::parse(
        &std::fs::read(root.join("lod-meta.json")).expect("manifest"),
        "https://capture/lod-meta.json",
        "packed".into(),
        "packed".into(),
    )
    .unwrap();
    stream.storage = SourceStorage::Packed;
    let settings = SogLodSettings {
        splat_budget: 5_600_000,
        cache_bytes: 256 * 1024 * 1024,
        ..default()
    };
    let cameras = [
        Vec3::new(-2.01, 0.127, -6.347),
        Vec3::new(8., 1., -35.),
        Vec3::new(-2.01, 0.127, -6.347),
    ];
    let mut selections = Vec::new();
    for (phase, eye) in cameras.into_iter().enumerate() {
        let mut settled = false;
        for tick in 0..200 {
            let view = selection::View {
                eye,
                forward: Vec3::NEG_Z,
                fov_scale: 1.,
                clip_from_local: None,
                ..Vec3::ZERO.into()
            };
            stream.tick(
                view,
                (phase * 200 + tick) as f64,
                &settings,
                6_000_000,
                Some(settings.cache_bytes as u64),
                |url, _, _, now| load(&root, url.strip_prefix("https://capture/").unwrap(), now),
            );
            // This is only the scheduler contract; the offscreen integration
            // separately proves real generation readiness and pixels.
            stream.submitted = stream.active.clone();
            stream.displayed = stream.active.clone();
            let status = stream.status();
            assert!(
                status.cache_ready_bytes + status.cache_reserved_bytes
                    <= settings.cache_bytes as u64
            );
            assert!(
                stream
                    .entries
                    .iter()
                    .filter_map(|e| e.ready.as_ref())
                    .all(|c| c.cloud.packed().is_ok())
            );
            if !stream.pending_work {
                settled = true;
                break;
            }
        }
        assert!(settled, "phase {phase}: {:?}", stream.last_error);
        let status = stream.status();
        eprintln!(
            "phase {phase}: displayed={} retained={} limit={}",
            status.displayed, status.cache_ready_bytes, status.effective
        );
        assert!(
            status.displayed > 1_044_448,
            "packed data should escape the old SH3 float ceiling"
        );
        assert!(stream.last_error.is_none(), "{:?}", stream.last_error);
        selections.push(stream.active.clone());
    }
    assert_ne!(
        selections[0], selections[1],
        "moving must change selected ranges"
    );
}
