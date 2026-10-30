//! Environment coverage is independent of tree-leaf visibility.
use super::*;

#[test]
fn environment_remains_selected_when_all_tree_leaves_are_offscreen() {
    let mut stream = tiny_stream();
    stream.manifest.environment = Some("environment/meta.json".into());
    stream.entries.push(Entry {
        ready: Some(chunk::DecodedChunk {
            cloud: vec![Gaussian3d::default()].into(),
            count: 1,
        }),
        ..default()
    });
    let view = selection::View {
        clip_from_local: Some(Mat4::orthographic_rh(8., 10., -2., 2., -2., 2.)),
        ..Vec3::ZERO.into()
    };
    let settings = SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    };
    stream.tick(view, 0., &settings, 1000, None, |_, _, _, _| {
        panic!("resident sources must not download")
    });
    assert_eq!(
        stream.active,
        [
            SplatRange {
                file: 0,
                offset: 0,
                count: 1
            },
            SplatRange {
                file: 1,
                offset: 0,
                count: 1
            }
        ]
    );
    assert!(stream.entries[0].ready.is_some());
}

#[test]
fn offscreen_coverage_stays_ready_with_unrenderable_declared_environment() {
    for (state, entry) in [
        (
            "unready",
            Entry {
                attempts: 1,
                retry_at: 1000.,
                ..default()
            },
        ),
        (
            "failed",
            Entry {
                attempts: 3,
                retry_at: 7.,
                ..default()
            },
        ),
        (
            "zero",
            Entry {
                ready: Some(chunk::DecodedChunk {
                    cloud: default(),
                    count: 0,
                }),
                ..default()
            },
        ),
    ] {
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
        {
            let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
            stream.manifest.environment = Some("environment/meta.json".into());
            stream.entries.push(entry);
        }
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert!(!stream.active.is_empty(), "{state}");
        assert!(stream.snapshot.is_visible(), "{state}");
        assert!(stream.snapshot.is_pending(), "{state}");
        assert!(app.world().get_entity(old).is_ok(), "{state}");
        assert!(
            children
                .iter()
                .all(|entity| app.world().get_entity(*entity).is_ok()),
            "{state}"
        );
        // Transport retry scheduling resumes after the renderer acknowledges
        // the pending coverage. Merely turning away must not cancel that wait.
        let pending = *children.iter().find(|&&entity| entity != old).unwrap();
        app.world()
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .insert(pending);
        app.update();
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        let environment = &stream.entries[1];
        match state {
            "unready" => {
                assert_eq!(environment.attempts, 1);
                assert_eq!(environment.retry_at, 1000.);
                assert!(stream.pending_work);
            }
            "failed" => {
                assert_eq!(environment.attempts, 3);
                assert_eq!(environment.retry_at, 7.);
                assert!(!stream.pending_work);
            }
            "zero" => {
                assert_eq!(environment.ready.as_ref().unwrap().count, 0);
                assert!(!stream.pending_work);
            }
            _ => unreachable!(),
        }
        assert!(
            stream.entries[0].ready.is_some(),
            "{state}: coarse cache must stay ready"
        );
    }
}

#[test]
fn ready_nonempty_environment_retains_pending_generations_when_tree_is_offscreen() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        visibility: SogVisibilityPolicy::Frustum,
        ..default()
    });
    {
        let mut stream = app.world_mut().get_mut::<SogStream>(root).unwrap();
        stream.manifest.environment = Some("environment/meta.json".into());
        stream.entries.push(Entry {
            ready: Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default()].into(),
                count: 1,
            }),
            ..default()
        });
    }
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .get_mut::<SogStream>(root)
        .unwrap()
        .submitted
        .clear();
    advance_stream(&mut app);
    let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
    assert_eq!(children.len(), 2);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from(Transform::from_xyz(100., 0., 0.)));
    advance_stream(&mut app);
    let stream = app.world().get::<SogStream>(root).unwrap();
    assert!(stream.snapshot.is_visible());
    assert!(stream.snapshot.is_pending());
    assert!(app.world().get_entity(old).is_ok());
    assert!(
        children
            .into_iter()
            .all(|entity| app.world().get_entity(entity).is_ok())
    );
    assert!(
        stream
            .active
            .iter()
            .any(|range| range.file == 1 && range.count == 1)
    );
}
