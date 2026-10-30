//! Screen-space and reference policies over live camera views.
use super::*;

#[test]
fn default_policy_stops_refining_small_regions_without_filling_the_budget() {
    let mut stream = pixel_stream();
    let settings = SogLodSettings {
        splat_budget: 100_000,
        ..default()
    };
    let view = selection::View {
        pixels: Some(screen_space::PixelView {
            clip_from_local: Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 1000.),
            viewport: Vec2::splat(200.),
        }),
        ..Vec3::ZERO.into()
    };
    stream.tick(view, 0., &settings, 100_000, None, |_, _, _, _| {
        panic!("cached")
    });
    assert_eq!(stream.active, [stream.manifest.leaves[0].lods[2].1]);
    assert_eq!(
        stream.active.iter().map(|range| range.count).sum::<usize>(),
        625
    );
}

#[test]
fn screen_space_frustum_keeps_render_ready_coarse_coverage_on_turn_and_return() {
    use crate::streaming::SogLodPolicy;
    for pending in [false, true] {
        let (mut app, root, camera) = stream_app(SogLodSettings {
            lod_policy: SogLodPolicy::ScreenSpace,
            visibility: SogVisibilityPolicy::All,
            ..default()
        });
        let old = activate_snapshot(&mut app, root);
        if pending {
            app.world_mut()
                .get_mut::<SogStream>(root)
                .unwrap()
                .submitted
                .clear();
            advance_stream(&mut app);
            assert!(
                app.world()
                    .get::<SogStream>(root)
                    .unwrap()
                    .snapshot
                    .is_pending()
            );
        }
        let children: Vec<_> = app.world().get::<Children>(root).unwrap().iter().collect();
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from(Transform::from_rotation(
                Quat::from_rotation_y(std::f32::consts::PI),
            )));
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert_eq!(stream.active.len(), 1, "offscreen coarse stays selected");
        assert_eq!(stream.snapshot.is_pending(), pending);
        assert!(
            stream.entries[0].ready.is_some(),
            "retain reusable coverage"
        );
        assert!(
            children
                .iter()
                .all(|entity| app.world().get_entity(*entity).is_ok())
        );
        assert!(app.world().get_entity(old).is_ok());

        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::IDENTITY);
        advance_stream(&mut app);
        let stream = app.world().get::<SogStream>(root).unwrap();
        assert_eq!(stream.active.len(), 1);
        assert!(stream.entries.iter().all(|entry| entry.loading.is_none()));
        assert_eq!(
            stream.snapshot.is_pending(),
            pending,
            "turn must not resubmit unchanged coverage"
        );
    }
}

#[test]
fn reference_all_keeps_offscreen_selection_after_policy_switch() {
    use crate::streaming::SogLodPolicy;
    let mut stream = pixel_stream();
    let mut settings = SogLodSettings {
        lod_policy: SogLodPolicy::ScreenSpace,
        splat_budget: 100_000,
        ..default()
    };
    let view = selection::View {
        pixels: Some(screen_space::PixelView {
            clip_from_local: Mat4::orthographic_rh(20., 30., -5., 5., 0.1, 100.),
            viewport: Vec2::splat(200.),
        }),
        ..Vec3::ZERO.into()
    };
    stream.tick(view, 0., &settings, 100_000, None, |_, _, _, _| {
        panic!("cached")
    });
    assert_eq!(
        stream.active,
        [stream.manifest.leaves[0].lods.last().unwrap().1]
    );
    settings.lod_policy = SogLodPolicy::ReferenceDistance;
    stream.tick(view, 1., &settings, 100_000, None, |_, _, _, _| {
        panic!("cached")
    });
    assert_eq!(stream.active.len(), 1);
    assert_eq!(stream.active[0], stream.manifest.leaves[0].lods[0].1);
    assert_eq!(settings.visibility, SogVisibilityPolicy::All);
}

#[test]
fn screen_space_stream_coarsens_and_reuses_fine_cache_on_return() {
    use crate::streaming::SogLodPolicy;
    let mut stream = pixel_stream();
    let settings = SogLodSettings {
        lod_policy: SogLodPolicy::ScreenSpace,
        splat_budget: 100_000,
        ..default()
    };
    for (step, (span, expected)) in [(2., 0), (5., 2), (20., 5), (2., 0)]
        .into_iter()
        .enumerate()
    {
        let view = selection::View {
            pixels: Some(screen_space::PixelView {
                clip_from_local: Mat4::orthographic_rh(
                    -span / 2.,
                    span / 2.,
                    -span / 2.,
                    span / 2.,
                    0.1,
                    1000.,
                ),
                viewport: Vec2::splat(200.),
            }),
            ..Vec3::ZERO.into()
        };
        stream.tick(view, step as f64, &settings, 100_000, None, |_, _, _, _| {
            panic!("reuse resident source")
        });
        let chosen = stream.manifest.leaves[0]
            .lods
            .iter()
            .find(|(_, range)| stream.active.contains(range))
            .expect("coverage")
            .0;
        assert_eq!(chosen, expected);
        assert!(
            !stream.admission.selection_limited,
            "quality target is not a budget failure"
        );
        stream.submitted = stream.active.clone();
        stream.displayed = stream.active.clone();
        assert_eq!(
            overlay::refinement_state(&stream, 0, chosen),
            super::overlay::RefinementState::Settled
        );
        assert!(
            stream.entries.iter().all(|entry| entry.ready.is_some()),
            "unpressured cache retains return-path detail"
        );
    }
}

#[test]
fn screen_space_runtime_reads_main_camera_viewport_and_live_transform() {
    use crate::streaming::SogLodPolicy;
    let (mut app, root, camera) = stream_app(SogLodSettings {
        lod_policy: SogLodPolicy::ScreenSpace,
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert((pixel_stream(), GlobalTransform::IDENTITY));
    app.world_mut()
        .entity_mut(camera)
        .insert(Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.,
            ..default()
        }));
    app.world_mut().spawn((
        Camera2d,
        Camera {
            viewport: Some(bevy::camera::Viewport {
                physical_size: UVec2::splat(8000),
                ..default()
            }),
            ..default()
        },
    ));
    for (step, (pixels, z, expected)) in [(200, 0., 2), (2000, 0., 0), (2000, 100., 2)]
        .into_iter()
        .enumerate()
    {
        app.world_mut()
            .get_mut::<Camera>(camera)
            .expect("camera")
            .viewport = Some(bevy::camera::Viewport {
            physical_size: UVec2::splat(pixels),
            ..default()
        });
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from_translation(Vec3::new(0., 0., z)));
        // Exercise the live system's projection/viewport source independently
        // of renderer-readiness (covered by the generation handoff tests).
        {
            let mut stream = app.world_mut().get_mut::<SogStream>(root).expect("stream");
            stream.snapshot = SnapshotState::default();
            stream.next_update = 0.;
        }
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_secs(1));
        app.update();
        assert_eq!(
            app.world()
                .get::<SogStream>(root)
                .expect("stream")
                .quality_history,
            [expected],
            "step {step}"
        );
    }
}
