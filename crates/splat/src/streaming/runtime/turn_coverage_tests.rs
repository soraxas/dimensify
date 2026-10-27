//! Render-ready coverage on a 180-degree turn, before the next streaming tick.
use super::*;
use crate::renderer::source::SourceStorage;

#[test]
#[ignore = "native offscreen GPU turn coverage; no window; run explicitly"]
fn packed_turn_has_coarse_pixels_before_selection_or_download_updates() {
    turn_coverage(false, false);
}

#[test]
#[ignore = "native offscreen paged GPU turn coverage; no window; run explicitly"]
fn paged_turn_has_coarse_pixels_before_selection_or_download_updates() {
    turn_coverage(true, false);
}

#[test]
#[ignore = "native offscreen compacted GPU turn coverage; no window; run explicitly"]
fn compacted_turn_has_coarse_pixels_before_selection_or_download_updates() {
    turn_coverage(false, true);
}

fn turn_coverage(pages: bool, compaction: bool) {
    let (mut app, target, camera) = render_app(3);
    app.insert_resource(SogRendererBackend::Packed);
    app.world_mut()
        .resource_mut::<SogRuntimeSettings>()
        .compaction = compaction;
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let mut data = SogStream::parse(
        br#"{"lodLevels":4,"filenames":["front-coarse/meta.json","back-coarse/meta.json","front-fine/meta.json","back-fine/meta.json"],
        "tree":{"bound":{"min":[-1.21,-0.91,-2.61],"max":[1.21,0.91,2.61]},"children":[
          {"bound":{"min":[-1.21,-0.91,-2.61],"max":[1.21,0.91,-1.39]},"lods":{
            "0":{"file":2,"offset":0,"count":2},"3":{"file":0,"offset":0,"count":1}}},
          {"bound":{"min":[-1.21,-0.91,1.39],"max":[1.21,0.91,2.61]},"lods":{
            "0":{"file":3,"offset":0,"count":2},"3":{"file":1,"offset":0,"count":1}}}
        ]}}"#,
        "https://unused.invalid/lod-meta.json",
        "turn".into(),
        "turn".into(),
    ).unwrap();
    if pages {
        data.storage = SourceStorage::Packed;
        data.enable_pages().unwrap();
        app.world_mut().resource_mut::<SogLodSettings>().cache_bytes = 16 << 20;
    }
    for (file, z) in [(0, -2.), (1, 2.)] {
        data.entries[file].loading =
            Some(chunk(Vec3::new(0., 0., z), false, now).with_storage(SourceStorage::Packed));
    }
    for entry in &mut data.entries {
        entry.retry_at = f64::INFINITY; // Deliberately no network.
    }
    let root = app
        .world_mut()
        .spawn((data, Transform::IDENTITY, Visibility::Inherited))
        .id();
    until(&mut app, "front coarse displayed", |app| {
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .displayed
            .iter()
            .any(|r| r.file == 0)
    });
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    app.world_mut().get_mut::<SogStream>(root).unwrap().entries[2].loading =
        Some(chunk(Vec3::new(0., 0., -2.), true, now).with_storage(SourceStorage::Packed));
    until(&mut app, "front fine displayed", |app| {
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .displayed
            .iter()
            .any(|r| r.file == 2)
    });
    assert!(coloured_pixels(&capture(&mut app, &target), 1) > 10);
    let (generation, submitted) = {
        let mut data = app.world_mut().get_mut::<SogStream>(root).unwrap();
        data.next_update = f64::INFINITY;
        (data.generation, data.submitted.clone())
    };
    app.world_mut()
        .entity_mut(camera)
        .insert(Transform::IDENTITY.looking_to(Vec3::Z, Vec3::Y));
    let back = capture(&mut app, &target);
    let data = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        data.generation, generation,
        "test must not hide the hole with a streaming tick"
    );
    assert_eq!(data.submitted, submitted);
    assert!(data.entries.iter().all(|entry| entry.loading.is_none()));
    assert!(
        coloured_pixels(&back, 0) > 10,
        "turn must reveal already-render-ready coarse coverage"
    );
    assert_eq!(
        coloured_pixels(&back, 1),
        0,
        "front fine detail is still frustum clipped"
    );

    // Deliver newly visible fine detail. It must replace, never overlay, coarse.
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    {
        let mut data = app.world_mut().get_mut::<SogStream>(root).unwrap();
        data.next_update = 0.;
        data.entries[3].loading =
            Some(chunk(Vec3::new(0., 0., 2.), true, now).with_storage(SourceStorage::Packed));
    }
    until(&mut app, "back fine displayed", |app| {
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .displayed
            .iter()
            .any(|r| r.file == 3)
    });
    let fine = capture(&mut app, &target);
    assert!(coloured_pixels(&fine, 1) > 10);
    assert_eq!(
        coloured_pixels(&fine, 0),
        0,
        "coarse must not double blend with fine"
    );
    let data = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        data.displayed.iter().map(|r| r.file).collect::<Vec<_>>(),
        [0, 3]
    );
    assert_eq!(data.displayed.iter().map(|r| r.count).sum::<usize>(), 3);
}
