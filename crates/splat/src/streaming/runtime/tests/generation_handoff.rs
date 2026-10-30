//! Renderer generation handoff: readiness, failed replacements and camera turns.
use super::*;

#[test]
fn replacement_waits_for_gpu_readiness_and_despawning_cleans_progress() {
    let mut app = App::new();
    renderer::readiness::install(&mut app);
    // Exercise stream policy/readiness with CPU fixtures, independently of
    // the production packed renderer (covered by packed_handoff_tests).
    app.insert_resource(renderer::SogRendererBackend::Snapshot);
    app.init_resource::<Assets<bevy_gaussian_splatting::sort::SortedEntries>>();
    app.init_resource::<Time<Real>>()
        .insert_resource(SogLodSettings {
            lod_policy: SogLodPolicy::ReferenceDistance,
            ..default()
        })
        .init_resource::<Assets<PlanarGaussian3d>>()
        .init_resource::<ReadySnapshots>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
        .add_plugins((
            bevy::transform::TransformPlugin,
            bevy::camera::visibility::VisibilityPlugin,
        ))
        .add_systems(
            PostUpdate,
            update_sog_streams
                .after(bevy::transform::TransformSystems::Propagate)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
        );
    app.world_mut().spawn((
        StreamingCamera,
        Camera3d::default(),
        Projection::default(),
        GlobalTransform::IDENTITY,
    ));
    let stream = tiny_stream();
    let root_transform =
        Transform::from_xyz(4.0, 2.0, -3.0).with_rotation(Quat::from_rotation_z(0.7));
    let root = app
        .world_mut()
        .spawn((
            stream,
            root_transform,
            Visibility::Inherited,
            Name::new("GaussianCloud: test"),
        ))
        .id();
    app.update();
    let old = app.world().get::<Children>(root).unwrap()[0];
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(old);
    app.update();
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_visible()
    );
    // Force another selection submission without network activity.
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .submitted
        .clear();
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs(1));
    app.update();
    let pending = app
        .world()
        .get::<Children>(root)
        .unwrap()
        .iter()
        .find(|entity| *entity != old)
        .unwrap();
    assert_eq!(
        *app.world().get::<Visibility>(pending).unwrap(),
        Visibility::Hidden
    );
    assert!(app.world().get_entity(old).is_ok());
    app.update();
    assert!(app.world().get_entity(old).is_ok());
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(pending);
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_secs(1));
    app.update();
    assert!(app.world().get_entity(old).is_err());
    let snapshot_transform = app.world().get::<GlobalTransform>(pending).unwrap();
    assert!(
        snapshot_transform
            .affine()
            .abs_diff_eq(GlobalTransform::from(root_transform).affine(), 1e-6,)
    );
    assert!(
        app.world()
            .get::<InheritedVisibility>(pending)
            .unwrap()
            .get()
    );
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, With<crate::streaming::runtime::SogStream>>();
    assert_eq!(query.iter(app.world()).collect::<Vec<_>>(), [root]);
    assert_eq!(
        *app.world().get::<Visibility>(pending).unwrap(),
        Visibility::Inherited
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_visible()
    );
    assert!(
        !app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    app.world_mut().despawn(root);
    app.update();
    assert!(app.world().get_entity(pending).is_err());
}

#[test]
fn streaming_uses_live_main_camera_fov_not_the_ui_camera() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert(GlobalTransform::from_translation(Vec3::new(6., 0., -10.)));
    app.world_mut().spawn((Camera2d, GlobalTransform::IDENTITY));
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .diagnostics = Some(Vec::new());
    let perspective = |degrees: f32| {
        Projection::Perspective(PerspectiveProjection {
            fov: degrees.to_radians(),
            aspect_ratio: 1.0,
            ..default()
        })
    };
    app.world_mut().entity_mut(camera).insert(perspective(20.));
    activate_snapshot(&mut app, root);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .diagnostics
            .as_ref()
            .unwrap()[0]
            .culled
    );
    app.world_mut().entity_mut(camera).insert(perspective(120.));
    advance_stream(&mut app);
    assert!(
        !app.world()
            .get::<SogStream>(root)
            .unwrap()
            .diagnostics
            .as_ref()
            .unwrap()[0]
            .culled
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .active
            .len(),
        1,
        "changing the main camera FOV must update visibility without reloading the scene"
    );
    // Narrowing again stops refinement eligibility, not coarse coverage.
    app.world_mut().entity_mut(camera).insert(perspective(20.));
    advance_stream(&mut app);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .diagnostics
            .as_ref()
            .unwrap()[0]
            .culled
    );
}

#[test]
fn failed_replacement_keeps_render_ready_generation() {
    let (mut app, root, _) = stream_app(default());
    let old = activate_snapshot(&mut app, root);
    let original = vec![SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    }];
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        stream.manifest.leaves[0].lods[0].1.count = 2;
        stream.manifest.file_counts[0] = 2;
        // A genuine adapter validation failure, without a GPU/device or mock.
    }
    advance_stream(&mut app);
    assert!(app.world().get_entity(old).is_ok());
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    assert!(
        !app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .loading_task()
            .is_some_and(|task| task.state == ProgressTaskState::Error)
    );
    // The same failure on another tick must keep its error and old visible child.
    advance_stream(&mut app);
    assert!(app.world().get_entity(old).is_ok());
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .loading_task()
            .is_some_and(|task| task.state == ProgressTaskState::Error)
    );
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        let chunk = stream.entries[0].ready.as_mut().unwrap();
        chunk.cloud = vec![Gaussian3d::default(); 2].into();
        chunk.count = 2;
    }
    advance_stream(&mut app);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    assert_eq!(app.world().get::<Children>(root).unwrap().len(), 2);
    assert!(app.world().get_entity(old).is_ok());
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        original
    );
    let pending = app
        .world()
        .get::<Children>(root)
        .expect("children")
        .iter()
        .find(|entity| *entity != old)
        .expect("replacement snapshot");
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Hidden)
    );
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .expect("readiness")
        .insert(pending);
    app.update();
    assert!(app.world().get_entity(old).is_err());
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world()
            .get::<SogStream>(root)
            .expect("stream")
            .displayed,
        [SplatRange {
            file: 0,
            offset: 0,
            count: 2
        }]
    );
}

#[test]
fn offscreen_coarse_snapshot_stays_render_ready_without_resubmission() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(stream.active.len(), 1);
    assert!(stream.snapshot.is_visible());
    assert!(!stream.snapshot.is_pending());
    assert!(app.world().get_entity(old).is_ok());
    assert!(stream.entries[0].ready.is_some());
    assert!(stream.loading_task().is_none());
}

#[test]
fn camera_turn_reuses_cached_coarse_coverage_while_finer_chunk_fails() {
    let mut stream = SogStream::parse(
        br#"{
        "lodLevels":2,"filenames":["coarse/meta.json","left/meta.json","right/meta.json"],
        "tree":{"bound":{"min":[-10,-1,-1],"max":[10,1,1]},"children":[
            {"bound":{"min":[-10,-1,-1],"max":[-8,1,1]},"lods":{
                "0":{"file":1,"offset":0,"count":8},"1":{"file":0,"offset":0,"count":2}}},
            {"bound":{"min":[8,-1,-1],"max":[10,1,1]},"lods":{
                "0":{"file":2,"offset":0,"count":8},"1":{"file":0,"offset":2,"count":2}}}
        ]}}"#,
        "https://example.test/lod-meta.json",
        "test".into(),
        "test".into(),
    )
    .unwrap();
    stream.entries[0].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default(); 4].into(),
        count: 4,
    });
    let settings = SogLodSettings {
        splat_budget: 10,
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    };
    let left = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(-10., -8., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let right = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(8., 10., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let mut requests = Vec::new();
    stream.tick(left, 0., &settings, 1000, None, |url, _, _, now| {
        requests.push(url);
        failed_captured_chunk(now)
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 0,
                offset: 2,
                count: 2
            }
        ]
    );
    assert_eq!(requests, ["https://example.test/left/meta.json"]);
    // Exhaust the failed finer file. Its fallback remains visible and coarse stays resident.
    stream.entries[1].loading = None;
    stream.entries[1].attempts = 3;
    stream.tick(left, 10., &settings, 1000, None, |_, _, _, _| {
        panic!("exhausted file must not retry")
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 0,
                offset: 2,
                count: 2
            }
        ]
    );
    stream.tick(right, 20., &settings, 1000, None, |url, _, _, now| {
        requests.push(url);
        failed_captured_chunk(now)
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 0,
                offset: 2,
                count: 2
            }
        ]
    );
    assert_eq!(requests[1], "https://example.test/right/meta.json");
    assert!(stream.entries[0].ready.is_some());
    stream.entries[2].loading = None;
    stream.entries[2].ready = Some(chunk::DecodedChunk {
        cloud: vec![Gaussian3d::default(); 8].into(),
        count: 8,
    });
    stream.tick(right, 30., &settings, 1000, None, |_, _, _, _| {
        panic!("cached sources must not redownload")
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 2
            },
            SplatRange {
                file: 2,
                offset: 0,
                count: 8
            }
        ]
    );
}

#[test]
fn camera_turn_preserves_current_and_unready_pending_coverage() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .submitted
        .clear();
    advance_stream(&mut app);
    let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
    assert_eq!(children.len(), 2);
    assert!(
        app.world()
            .get::<SogStream>(root)
            .unwrap()
            .snapshot
            .is_pending()
    );
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(!stream.active.is_empty());
    assert!(stream.snapshot.is_visible());
    assert!(stream.snapshot.is_pending());
    assert!(app.world().get_entity(old).is_ok());
    assert!(
        children
            .into_iter()
            .all(|entity| app.world().get_entity(entity).is_ok())
    );
}
