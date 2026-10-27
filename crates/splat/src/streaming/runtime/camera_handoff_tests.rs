//! Camera → policy → real snapshot composition → readiness acknowledgement.
//! No GPU is fabricated: these tests explicitly drive its acknowledgement seam.
use super::*;
use crate::renderer::readiness::ReadySnapshots;
#[cfg(test)]
use crate::streaming::SogLodPolicy;
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d, PlanarGaussian3dHandle};
use std::time::Duration;

pub const LEFT: SplatRange = SplatRange {
    file: 1,
    offset: 0,
    count: 6,
};
pub const RIGHT: SplatRange = SplatRange {
    file: 2,
    offset: 0,
    count: 6,
};
pub const LEFT_COARSE: SplatRange = SplatRange {
    file: 0,
    offset: 0,
    count: 2,
};
pub const RIGHT_COARSE: SplatRange = SplatRange {
    file: 0,
    offset: 2,
    count: 2,
};

fn perspective(degrees: f32) -> Projection {
    Projection::Perspective(PerspectiveProjection {
        fov: degrees.to_radians(),
        aspect_ratio: 1.0,
        ..default()
    })
}

/// Drive the deterministic camera-to-selection acceptance fixture.
pub fn fixture() -> (App, Entity, Entity) {
    let mut stream = SogStream::parse(
        br#"{
      "lodLevels":2,"filenames":["coarse/meta.json","left/meta.json","right/meta.json"],
      "tree":{"bound":{"min":[-6,-1,-11],"max":[6,1,-9]},"children":[
        {"bound":{"min":[-6,-1,-11],"max":[-4,1,-9]},"lods":{
          "0":{"file":1,"offset":0,"count":6},"1":{"file":0,"offset":0,"count":2}}},
        {"bound":{"min":[4,-1,-11],"max":[6,1,-9]},"lods":{
          "0":{"file":2,"offset":0,"count":6},"1":{"file":0,"offset":2,"count":2}}}
      ]}}"#,
        "https://unused.invalid/lod-meta.json",
        "camera fixture".into(),
        "camera-fixture".into(),
    )
    .expect("valid fixture");
    for (file, count) in [4, 6, 6].into_iter().enumerate() {
        let mut points = vec![Gaussian3d::default(); 32];
        for (i, point) in points.iter_mut().take(count).enumerate() {
            let left = file == 1 || (file == 0 && i < 2);
            point.position_visibility.position = [if left { -5.0 } else { 5.0 }, 0.0, -10.0];
        }
        stream.entries[file].ready = Some(chunk::DecodedChunk {
            cloud: points.into(),
            count,
        });
    }
    let mut app = App::new();
    renderer::readiness::install(&mut app);
    app.insert_resource(renderer::SogRendererBackend::Snapshot);
    app.init_resource::<Assets<bevy_gaussian_splatting::sort::SortedEntries>>()
        .init_resource::<Assets<PlanarGaussian3d>>()
        .init_resource::<Time<Real>>()
        .insert_resource(SogLodSettings {
            visibility: SogVisibilityPolicy::Frustum,
            splat_budget: 16,
            cache_bytes: 96 * chunk::DecodedChunk::bytes_per_splat(),
            ..default()
        })
        .add_systems(
            PostUpdate,
            update_sog_streams.after(TransformSystems::Propagate),
        );
    let camera = app
        .world_mut()
        .spawn((
            StreamingCamera,
            Camera3d::default(),
            perspective(20.),
            GlobalTransform::from_translation(Vec3::new(-5., 0., 0.)),
        ))
        .id();
    // A moving UI camera must not feed streaming or stall single-main-camera lookup.
    app.world_mut().spawn((
        Camera2d,
        GlobalTransform::from_translation(Vec3::splat(1000.)),
    ));
    let root = app
        .world_mut()
        .spawn((stream, GlobalTransform::IDENTITY))
        .id();
    (app, root, camera)
}

/// Drive the deterministic camera-to-selection acceptance fixture.
pub fn tick(app: &mut App) {
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(250));
    app.update();
}

fn stream(app: &App, root: Entity) -> &SogStream {
    app.world().get::<SogStream>(root).expect("stream")
}

/// Drive the deterministic camera-to-selection acceptance fixture.
pub fn acknowledge(app: &mut App, root: Entity, expected: &[SplatRange]) -> Entity {
    let pending = app
        .world()
        .get::<Children>(root)
        .expect("snapshot children")
        .iter()
        .find(|entity| app.world().get::<Visibility>(*entity) == Some(&Visibility::Hidden))
        .expect("hidden pending snapshot");
    // Verify actual composed attributes, not only a submitted counter or matrix.
    let handle = app
        .world()
        .get::<PlanarGaussian3dHandle>(pending)
        .expect("cloud handle");
    let cloud = app
        .world()
        .resource::<Assets<PlanarGaussian3d>>()
        .get(&handle.0)
        .expect("composed asset");
    let xs: Vec<f32> = cloud
        .position_visibility
        .iter()
        .take(expected.iter().map(|range| range.count).sum())
        .map(|point| point.position[0])
        .collect();
    let expected_xs: Vec<f32> = expected
        .iter()
        .flat_map(|range| {
            std::iter::repeat_n(
                if range.file == 1 || (range.file == 0 && range.offset == 0) {
                    -5.0
                } else {
                    5.0
                },
                range.count,
            )
        })
        .collect();
    assert_eq!(xs, expected_xs);
    app.world()
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .expect("readiness lock")
        .insert(pending);
    app.update();
    assert_eq!(stream(app, root).displayed, expected);
    assert_eq!(
        app.world().get::<Visibility>(pending),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world().get::<Children>(root).expect("children").len(),
        1
    );
    pending
}

/// Drive the deterministic camera-to-selection acceptance fixture.
pub fn assert_transition(
    app: &App,
    root: Entity,
    old: Entity,
    displayed: &[SplatRange],
    submitted: &[SplatRange],
) {
    let stream = stream(app, root);
    assert_eq!(stream.displayed, displayed);
    assert_eq!(stream.submitted, submitted);
    assert!(stream.snapshot.is_pending());
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    let status = stream.status();
    assert!(
        status.cache_ready_bytes + status.cache_reserved_bytes
            <= app.world().resource::<SogLodSettings>().cache_bytes as u64
    );
    assert!(
        stream.entries.iter().all(|entry| entry.loading.is_none()),
        "resident sources must not redownload"
    );
}

#[test]
fn default_screen_space_reselects_entering_view_at_the_next_policy_tick() {
    let (mut app, root, camera) = fixture();
    assert_eq!(
        app.world().resource::<SogLodSettings>().lod_policy,
        SogLodPolicy::ScreenSpace
    );
    app.world_mut().get_mut::<Camera>(camera).unwrap().viewport = Some(bevy::camera::Viewport {
        physical_size: UVec2::splat(800),
        ..default()
    });
    tick(&mut app);
    let old = acknowledge(&mut app, root, &[LEFT, RIGHT_COARSE]);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(5., 0., 0.)));
    // Selection is throttled to 5 Hz, but readiness is polled every frame.
    // Do not force next_update or clear any selection/cache state here.
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(100));
    app.update();
    assert_eq!(stream(&app, root).submitted, [LEFT, RIGHT_COARSE]);
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(Duration::from_millis(101));
    app.update();
    assert_transition(
        &app,
        root,
        old,
        &[LEFT, RIGHT_COARSE],
        &[LEFT_COARSE, RIGHT],
    );
    let right = acknowledge(&mut app, root, &[LEFT_COARSE, RIGHT]);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(-5., 0., 0.)));
    tick(&mut app);
    assert_transition(
        &app,
        root,
        right,
        &[LEFT_COARSE, RIGHT],
        &[LEFT, RIGHT_COARSE],
    );
    acknowledge(&mut app, root, &[LEFT, RIGHT_COARSE]);
}

#[test]
fn camera_translation_reselects_visible_ranges() {
    let (mut app, root, camera) = fixture();
    tick(&mut app);
    let left = acknowledge(&mut app, root, &[LEFT, RIGHT_COARSE]);
    let original_cap = app.world().resource::<SogLodSettings>().cache_bytes;
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(5., 0., 0.)));
    tick(&mut app);
    assert_transition(
        &app,
        root,
        left,
        &[LEFT, RIGHT_COARSE],
        &[LEFT_COARSE, RIGHT],
    );
    tick(&mut app); // No acknowledgement: old coverage and its ranges stay pinned.
    assert_transition(
        &app,
        root,
        left,
        &[LEFT, RIGHT_COARSE],
        &[LEFT_COARSE, RIGHT],
    );
    let right = acknowledge(&mut app, root, &[LEFT_COARSE, RIGHT]);
    assert!(app.world().get_entity(left).is_err());
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(-5., 0., 0.)));
    tick(&mut app);
    assert_transition(
        &app,
        root,
        right,
        &[LEFT_COARSE, RIGHT],
        &[LEFT, RIGHT_COARSE],
    );
    acknowledge(&mut app, root, &[LEFT, RIGHT_COARSE]);
    assert!(app.world().get_entity(right).is_err());
    assert_eq!(
        app.world().resource::<SogLodSettings>().cache_bytes,
        original_cap
    );
}

#[test]
fn fov_change_reselects_visible_ranges() {
    let (mut app, root, camera) = fixture();
    tick(&mut app);
    let narrow = acknowledge(&mut app, root, &[LEFT, RIGHT_COARSE]);
    app.world_mut().entity_mut(camera).insert(perspective(120.));
    tick(&mut app);
    assert_transition(&app, root, narrow, &[LEFT, RIGHT_COARSE], &[LEFT, RIGHT]);
    let wide = acknowledge(&mut app, root, &[LEFT, RIGHT]);
    app.world_mut().entity_mut(camera).insert(perspective(20.));
    tick(&mut app);
    assert_transition(&app, root, wide, &[LEFT, RIGHT], &[LEFT, RIGHT_COARSE]);
    acknowledge(&mut app, root, &[LEFT, RIGHT_COARSE]);
}

#[test]
fn live_camera_distance_moves_detail_even_when_leaf_culling_is_disabled() {
    let (mut app, root, camera) = fixture();
    {
        let mut settings = app.world_mut().resource_mut::<SogLodSettings>();
        settings.visibility = SogVisibilityPolicy::All;
        settings.lod_policy = SogLodPolicy::ReferenceDistance;
        settings.splat_budget = 8;
    }
    let left_coarse = SplatRange {
        file: 0,
        offset: 0,
        count: 2,
    };
    let right_coarse = SplatRange {
        file: 0,
        offset: 2,
        count: 2,
    };
    // The root is translated/rotated relative to the camera's world frame.
    // All visibility makes this a distance-input test, not a frustum test.
    let scene = Transform::from_xyz(50., 0., 20.)
        .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
    app.world_mut()
        .entity_mut(root)
        .insert(GlobalTransform::from(scene));
    let set_eye = |app: &mut App, x| {
        let local_camera = Transform::from_xyz(x, 0., -8.);
        app.world_mut()
            .entity_mut(camera)
            .insert(GlobalTransform::from(scene * local_camera));
    };
    set_eye(&mut app, -5.);
    tick(&mut app);
    let left = acknowledge(&mut app, root, &[LEFT, right_coarse]);
    set_eye(&mut app, 5.);
    tick(&mut app);
    assert_transition(
        &app,
        root,
        left,
        &[LEFT, right_coarse],
        &[left_coarse, RIGHT],
    );
    let right = acknowledge(&mut app, root, &[left_coarse, RIGHT]);
    set_eye(&mut app, -5.);
    tick(&mut app);
    assert_transition(
        &app,
        root,
        right,
        &[left_coarse, RIGHT],
        &[LEFT, right_coarse],
    );
    acknowledge(&mut app, root, &[LEFT, right_coarse]);
}
