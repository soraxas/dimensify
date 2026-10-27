//! Pixel-level reproduction using an actual audited kitchen leaf and its bound.
use super::*;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy_gaussian_splatting::io::sog::{decode_sog, parse_meta};

fn kitchen_leaf() -> (Vec<u8>, HashMap<String, Vec<u8>>, serde_json::Value) {
    let root = std::path::PathBuf::from(std::env::var("SOG_REPLAY_DIR").expect("saved fixture"));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("lod-meta.json")).expect("saved manifest"))
            .expect("manifest JSON");
    fn leaves<'a>(node: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        if let Some(children) = node["children"].as_array() {
            for child in children {
                leaves(child, out);
            }
        } else {
            out.push(node);
        }
    }
    let mut all = Vec::new();
    leaves(&manifest["tree"], &mut all);
    // Leaf 21 / LOD 1 includes the largest decoded 1-sigma overshoot from
    // the complete audit. Preserve every splat in this range, not just the worst.
    let leaf = all[21];
    let range: SplatRange = serde_json::from_value(leaf["lods"]["1"].clone()).expect("range");
    assert_eq!(
        (range.offset, range.count),
        (410786, 39),
        "fixture identity changed"
    );
    let path = manifest["filenames"][range.file]
        .as_str()
        .expect("filename");
    let meta_path = root.join(path);
    let mut raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&meta_path).expect("metadata"))
            .expect("metadata JSON");
    let source_meta = parse_meta(&serde_json::to_vec(&raw).expect("metadata bytes")).expect("SOG");
    let files: HashMap<_, _> = source_meta
        .required_files_for_decode(
            bevy_gaussian_splatting::material::spherical_harmonics::SH_COEFF_COUNT_PER_CHANNEL,
        )
        .into_iter()
        .map(|name| {
            (
                name.to_string(),
                std::fs::read(meta_path.parent().expect("directory").join(name)).expect(name),
            )
        })
        .collect();
    let source = decode_sog(&source_meta, &files).expect("production decode of original chunk");
    let mut cropped = HashMap::new();
    for (name, bytes) in &files {
        // The SH palette is shared, not indexed one pixel per splat.
        if source_meta
            .sh_n
            .as_ref()
            .is_some_and(|sh| sh.files.first() == Some(name))
        {
            cropped.insert(name.clone(), bytes.clone());
            continue;
        }
        let image = Image::from_buffer(
            bytes,
            ImageType::Format(ImageFormat::WebP),
            CompressedImageFormats::NONE,
            false,
            ImageSampler::default(),
            RenderAssetUsages::MAIN_WORLD,
        )
        .expect("lossless source WebP");
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        // Encoded attributes are bytes, not colours. Read RGBA without a
        // DynamicImage colour-format conversion (linear RGBA is unsupported).
        let rgba = image.data.expect("decoded RGBA bytes");
        let pixels = rgba[range.offset * 4..(range.offset + range.count) * 4].to_vec();
        let image = Image::new(
            Extent3d {
                width: range.count as u32,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        );
        let mut encoded = Cursor::new(Vec::new());
        image
            .try_into_dynamic()
            .expect("RGBA")
            .write_to(
                &mut encoded,
                ImageFormat::WebP.as_image_crate_format().expect("WebP"),
            )
            .expect("lossless subset");
        cropped.insert(name.clone(), encoded.into_inner());
    }
    raw["count"] = serde_json::json!(range.count);
    let meta = serde_json::to_vec(&raw).expect("subset metadata");
    let decoded =
        decode_sog(&parse_meta(&meta).expect("subset SOG"), &cropped).expect("subset decode");
    for i in 0..range.count {
        let row = range.offset + i;
        assert_eq!(
            source.position_visibility[row].position,
            decoded.position_visibility[i].position
        );
        assert_eq!(source.rotation[row].rotation, decoded.rotation[i].rotation);
        assert_eq!(
            source.scale_opacity[row].scale,
            decoded.scale_opacity[i].scale
        );
        assert_eq!(
            source.scale_opacity[row].opacity,
            decoded.scale_opacity[i].opacity
        );
        assert_eq!(
            source.spherical_harmonic[row].coefficients,
            decoded.spherical_harmonic[i].coefficients
        );
    }
    (meta, cropped, leaf["bound"].clone())
}

fn save_pixels(name: &str, pixels: &[u8]) {
    if let Ok(directory) = std::env::var("SOG_BOUNDS_CAPTURE_DIR") {
        Image::new(
            Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.to_vec(),
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
        .try_into_dynamic()
        .expect("capture")
        .save(std::path::Path::new(&directory).join(name))
        .expect("capture PNG");
    }
}

#[test]
#[ignore = "requires SOG_REPLAY_DIR and an offscreen native GPU; no window/network"]
fn real_kitchen_leaf_tail_survives_frustum_selection() {
    let (meta, files, bound) = kitchen_leaf();
    let (mut app, target, camera) = render_app(64);
    app.world_mut().entity_mut(camera).insert((
        Transform::from_xyz(24.8, 0.4, -31.),
        Projection::Perspective(PerspectiveProjection {
            fov: 60f32.to_radians(),
            near: 0.1,
            ..default()
        }),
    ));
    for (backend, compaction) in [
        (SogRendererBackend::Snapshot, false),
        (SogRendererBackend::GpuResident, false),
        (SogRendererBackend::GpuResident, true),
    ] {
        app.insert_resource(backend);
        app.world_mut()
            .resource_mut::<SogRuntimeSettings>()
            .compaction = compaction;
        app.world_mut().resource_mut::<SogLodSettings>().visibility = SogVisibilityPolicy::All;
        let bytes = serde_json::to_vec(&serde_json::json!({
            "lodLevels": 2, "filenames": ["subset/meta.json"],
            "tree": {"bound": bound, "lods": {"1": {"file": 0,"offset": 0,"count": 39}}}
        }))
        .expect("subset hierarchy");
        let mut stream = SogStream::parse(
            &bytes,
            "https://unused.invalid/lod-meta.json",
            "kitchen leaf 21".into(),
            "bounds probe".into(),
        )
        .expect("stream");
        let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
        stream.entries[0].loading = Some(ChunkLoad::captured(&meta, files.clone(), now));
        let root = app
            .world_mut()
            .spawn((stream, Transform::IDENTITY, Visibility::Inherited))
            .id();
        until(&mut app, "real leaf render-ready", |app| {
            app.world()
                .get::<SogStream>(root)
                .expect("stream")
                .displayed
                == [SplatRange {
                    file: 0,
                    offset: 0,
                    count: 39,
                }]
        });
        let baseline = capture(&mut app, &target);
        assert!(
            !empty_image(&baseline),
            "the source leaf must actually contribute visible pixels"
        );
        save_pixels(&format!("{backend:?}-{compaction}-all.png"), &baseline);
        app.world_mut().resource_mut::<SogLodSettings>().visibility = SogVisibilityPolicy::Frustum;
        app.world_mut()
            .get_mut::<SogStream>(root)
            .expect("stream")
            .next_update = 0.;
        app.update();
        until(&mut app, "frustum selection render-ready", |app| {
            let s = app.world().get::<SogStream>(root).expect("stream");
            s.active == s.displayed && !s.snapshot.is_pending()
        });
        let culled = capture(&mut app, &target);
        save_pixels(&format!("{backend:?}-{compaction}-frustum.png"), &culled);
        eprintln!(
            "kitchen leaf: backend={backend:?}, compaction={compaction}, All lit={}, Frustum lit={}",
            baseline
                .chunks_exact(4)
                .filter(|p| p[..3] != [0, 0, 0])
                .count(),
            culled
                .chunks_exact(4)
                .filter(|p| p[..3] != [0, 0, 0])
                .count()
        );
        assert!(
            baseline == culled,
            "frustum selection clipped real source pixels: {backend:?}, compaction={compaction}"
        );
        app.world_mut().entity_mut(root).despawn();
        app.update();
        assert!(
            empty_image(&capture(&mut app, &target)),
            "scene removal left stale pixels"
        );
    }
}
