use super::*;
use bevy::{camera::primitives::Aabb, prelude::*};
use renderer::{PointCloud, PointCloud3d, PointCloudChunk, PointCloudLoadTasks};

fn status_app() -> App {
    let mut app = App::new();
    app.init_resource::<Assets<PointCloud>>()
        .init_resource::<Assets<PointCloudChunk>>()
        .init_resource::<PointCloudLoadTasks>()
        .init_resource::<Time>()
        .add_message::<renderer::PointCloudLoadFailed>()
        .add_systems(Update, loading::update_status);
    app
}

#[test]
fn axes_are_explicit_and_preserve_source_translation() {
    let transform = Transform::from_xyz(1., 2., 3.);
    let request = LoadPointcloud {
        up: PointcloudUp::Z,
        transform,
        ..LoadPointcloud::mesh(Mesh::from(Cuboid::default()))
    };
    let converted = request.render_transform();
    assert!(
        converted
            .translation
            .abs_diff_eq(Vec3::new(1., 3., -2.), 1e-5)
    );
    let identity = LoadPointcloud {
        up: PointcloudUp::Y,
        ..request
    };
    assert_eq!(identity.render_transform(), transform);
}

#[test]
fn removing_a_cloud_retires_its_visibility_class_and_preserves_other_classes() {
    use bevy::camera::visibility::VisibilityClass;
    use std::any::TypeId;
    let mut app = App::new();
    app.add_plugins(renderer::PointCloudVisiblityPlugin);
    let entity = app.world_mut().spawn(PointCloud3d::default()).id();
    let cloud_class = TypeId::of::<PointCloud3d>();
    let host_class = TypeId::of::<Mesh3d>();
    app.world_mut()
        .get_mut::<VisibilityClass>(entity)
        .unwrap()
        .push(host_class);
    assert!(
        app.world()
            .get::<VisibilityClass>(entity)
            .unwrap()
            .contains(&cloud_class)
    );
    app.world_mut().entity_mut(entity).remove::<PointCloud3d>();
    let classes = app.world().get::<VisibilityClass>(entity).unwrap();
    assert!(!classes.contains(&cloud_class));
    assert!(classes.contains(&host_class));
    app.world_mut()
        .entity_mut(entity)
        .insert(PointCloud3d::default());
    assert_eq!(
        app.world()
            .get::<VisibilityClass>(entity)
            .unwrap()
            .iter()
            .filter(|id| **id == cloud_class)
            .count(),
        1
    );
}

#[test]
fn bounds_do_not_claim_drawable_content() {
    let mut app = status_app();
    let aabb = Aabb::from_min_max(Vec3::ZERO, Vec3::ONE);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PointCloud>>()
        .add(PointCloud {
            aabb: Some(aabb),
            ..default()
        });
    let entity = app
        .world_mut()
        .spawn((PointCloud3d(handle), PointcloudStatus::loading(0., 30.)))
        .id();
    app.update();
    let status = app.world().get::<PointcloudStatus>(entity).unwrap();
    assert!(status.bounds.is_some());
    assert_eq!(status.phase, PointcloudPhase::Preparing);
}

#[test]
fn metadata_deadline_fails_only_the_matching_cloud() {
    let mut app = status_app();
    let handle = app
        .world_mut()
        .resource::<Assets<PointCloud>>()
        .get_handle_provider()
        .reserve_handle()
        .typed();
    let entity = app
        .world_mut()
        .spawn((PointCloud3d(handle), PointcloudStatus::loading(0., 30.)))
        .id();
    let other = app
        .world_mut()
        .spawn((PointCloud3d::default(), PointcloudStatus::loading(31., 30.)))
        .id();
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(std::time::Duration::from_secs(31));
    app.update();
    assert_eq!(
        app.world().get::<PointcloudStatus>(entity).unwrap().phase,
        PointcloudPhase::Failed
    );
    assert_eq!(
        app.world().get::<PointcloudStatus>(other).unwrap().phase,
        PointcloudPhase::Metadata
    );
}

#[test]
fn late_results_do_not_recover_a_failed_request() {
    let mut app = status_app();
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PointCloud>>()
        .add(PointCloud::default());
    let mut status = PointcloudStatus::loading(0., 30.);
    status.phase = PointcloudPhase::Failed;
    status.error = Some("cancelled".into());
    let entity = app.world_mut().spawn((PointCloud3d(handle), status)).id();
    app.update();
    assert_eq!(
        app.world()
            .get::<PointcloudStatus>(entity)
            .unwrap()
            .error
            .as_deref(),
        Some("cancelled")
    );
}

#[test]
fn prepared_content_and_refinement_are_independent_per_cloud() {
    let mut app = status_app();
    let chunk = app
        .world_mut()
        .resource_mut::<Assets<PointCloudChunk>>()
        .add(PointCloudChunk {
            mesh_handle: Some(Handle::default()),
            vertex_buffer_size: 12,
            ..default()
        });
    let handle = app
        .world_mut()
        .resource_mut::<Assets<PointCloud>>()
        .add(PointCloud {
            topology: renderer::PointCloudTopology::Flat(chunk),
            ..default()
        });
    let other = app
        .world_mut()
        .resource_mut::<Assets<PointCloud>>()
        .add(PointCloud::default());
    app.world_mut()
        .resource_mut::<PointCloudLoadTasks>()
        .queue_load_request(
            other.id(),
            renderer::NodeId::null(),
            1_f32.into(),
            renderer::LoadRequestType::Chunk,
        );
    let ready = app
        .world_mut()
        .spawn((PointCloud3d(handle), PointcloudStatus::loading(0., 30.)))
        .id();
    let pending = app
        .world_mut()
        .spawn((PointCloud3d(other), PointcloudStatus::loading(0., 30.)))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<PointcloudStatus>(ready).unwrap().phase,
        PointcloudPhase::Ready
    );
    assert_eq!(
        app.world().get::<PointcloudStatus>(ready).unwrap().queued,
        0
    );
    assert_eq!(
        app.world().get::<PointcloudStatus>(pending).unwrap().queued,
        1
    );
}

#[test]
fn initial_failure_is_delivered_without_waiting_for_a_timeout() {
    let mut app = status_app();
    let handle = app
        .world()
        .resource::<Assets<PointCloud>>()
        .get_handle_provider()
        .reserve_handle()
        .typed();
    let id = handle.id();
    let entity = app
        .world_mut()
        .spawn((PointCloud3d(handle), PointcloudStatus::loading(0., 30.)))
        .id();
    app.world_mut()
        .write_message(renderer::PointCloudLoadFailed {
            id,
            error: "HTTP 403".into(),
        });
    app.update();
    assert_eq!(
        app.world()
            .get::<PointcloudStatus>(entity)
            .unwrap()
            .error
            .as_deref(),
        Some("HTTP 403")
    );
    app.world_mut().despawn(entity);
    app.world_mut()
        .write_message(renderer::PointCloudLoadFailed {
            id,
            error: "late failure".into(),
        });
    app.update();
}

#[test]
fn retired_handle_failure_cannot_fail_its_replacement() {
    let mut app = status_app();
    let old = app
        .world()
        .resource::<Assets<PointCloud>>()
        .get_handle_provider()
        .reserve_handle()
        .typed::<PointCloud>();
    let current = app
        .world_mut()
        .resource_mut::<Assets<PointCloud>>()
        .add(PointCloud::default());
    let entity = app
        .world_mut()
        .spawn((PointCloud3d(current), PointcloudStatus::loading(0., 30.)))
        .id();
    app.world_mut()
        .write_message(renderer::PointCloudLoadFailed {
            id: old.id(),
            error: "retired".into(),
        });
    app.update();
    let status = app.world().get::<PointcloudStatus>(entity).unwrap();
    assert_eq!(status.phase, PointcloudPhase::Preparing);
    assert!(status.error.is_none());
}

#[test]
fn invalid_replacement_retires_old_render_chunks_but_preserves_host_children() {
    let mut app = App::new();
    app.init_resource::<Assets<PointCloud>>()
        .init_resource::<Assets<renderer::StandardPointCloudMaterial>>()
        .init_resource::<renderer::PointCloudServer>()
        .init_resource::<Time>()
        .add_systems(Update, loading::load_requests);
    let cloud = app
        .world_mut()
        .spawn((
            PointCloud3d::default(),
            LoadPointcloud::potree("file:///unsupported"),
        ))
        .id();
    let chunk = app
        .world_mut()
        .spawn((ChildOf(cloud), renderer::PointCloudChunk3d::default()))
        .id();
    let host_child = app.world_mut().spawn(ChildOf(cloud)).id();
    app.update();
    assert!(app.world().get::<PointCloud3d>(cloud).is_none());
    assert!(app.world().get_entity(chunk).is_err());
    assert!(app.world().get_entity(host_child).is_ok());
    assert_eq!(
        app.world().get::<PointcloudStatus>(cloud).unwrap().phase,
        PointcloudPhase::Failed
    );
}

#[test]
fn scan_rgb_is_displayed_with_an_unlit_double_sided_material() {
    let mut app = App::new();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .init_resource::<Assets<PointCloud>>()
        .init_resource::<Assets<renderer::StandardPointCloudMaterial>>()
        .init_resource::<renderer::PointCloudServer>()
        .init_resource::<Time>()
        .add_systems(Update, loading::load_requests);
    let cloud = app
        .world_mut()
        .spawn(LoadPointcloud::mesh(Mesh::from(Cuboid::default())))
        .id();
    app.update();
    let material = app
        .world()
        .get::<renderer::PointCloudMaterial3d<renderer::StandardPointCloudMaterial>>(cloud)
        .unwrap();
    let material = app
        .world()
        .resource::<Assets<renderer::StandardPointCloudMaterial>>()
        .get(&material.0)
        .unwrap();
    assert!(material.unlit);
    assert_eq!(material.cull_mode, None);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_multifile_source_needs_no_octree_bin() {
    use potree::asset::PotreeAsset;
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("metadata.json"),
        r#"{
        "version":"2.0", "name":"fixture", "description":"", "points":1,
        "projection":"", "hierarchy":{"firstChunkSize":0,"stepSize":5,"depth":0},
        "offset":[0,0,0], "scale":[1,1,1], "spacing":1,
        "boundingBox":{"min":[0,0,0],"max":[1,1,1]},
        "encoding":"DEFAULT", "attributes":[], "octreeLayout":"multi-file"
    }"#,
    )
    .unwrap();
    std::fs::write(directory.path().join("hierarchy.bin"), []).unwrap();
    std::fs::create_dir(directory.path().join("octree")).unwrap();
    std::fs::write(directory.path().join("octree/r.bin"), b"node-points").unwrap();
    assert!(!directory.path().join("octree.bin").exists());
    let source =
        renderer::potree::PotreeAssetSource::<renderer::FileSource>::from_path(directory.path())
            .unwrap();
    futures::executor::block_on(async {
        source.read_metadata().await.unwrap();
        assert_eq!(
            source.read_octree_node("r", 0, 11).await.unwrap().as_ref(),
            b"node-points"
        );
    });
    // Single-file datasets still validate their backing file during metadata
    // loading, and retain byte-range reads after it becomes available.
    let metadata_path = directory.path().join("metadata.json");
    let metadata = std::fs::read_to_string(&metadata_path).unwrap();
    std::fs::write(
        &metadata_path,
        metadata.replace("multi-file", "single-file"),
    )
    .unwrap();
    let source =
        renderer::potree::PotreeAssetSource::<renderer::FileSource>::from_path(directory.path())
            .unwrap();
    assert!(futures::executor::block_on(source.read_metadata()).is_err());
    std::fs::write(directory.path().join("octree.bin"), b"single-points").unwrap();
    futures::executor::block_on(async {
        source.read_metadata().await.unwrap();
        assert_eq!(
            source.read_octree_node("r", 7, 6).await.unwrap().as_ref(),
            b"points"
        );
    });
}

struct FailingLoader;

impl renderer::OctreeLoader for FailingLoader {
    type Source = ();
    type Settings = ();
    type Hierarchy = ();
    type Error = bevy::ecs::error::BevyError;

    async fn from_source(_: (), _: ()) -> Result<Self, Self::Error> {
        Err("fixture metadata failure".into())
    }

    async fn load_initial_hierarchy(
        &self,
        _: &mut renderer::OctreeHierarchyBuilder<()>,
    ) -> Result<(), Self::Error> {
        unreachable!()
    }

    async fn load_chunk(&self, _: &()) -> Result<renderer::ChunkLoadResult, Self::Error> {
        unreachable!()
    }
}

#[test]
fn async_loader_failure_reaches_shared_status_through_the_renderer() {
    let mut app = status_app();
    app.add_plugins(bevy::app::TaskPoolPlugin::default())
        .init_resource::<renderer::PointCloudServer>()
        .init_resource::<renderer::PointCloudTracking>()
        .init_resource::<renderer::PointCloudInstances>()
        .add_systems(
            Update,
            renderer::handle_internal_point_cloud_events.before(loading::update_status),
        );
    let handle = app
        .world()
        .resource::<renderer::PointCloudServer>()
        .load::<FailingLoader>(());
    let entity = app
        .world_mut()
        .spawn((PointCloud3d(handle), PointcloudStatus::loading(0., 30.)))
        .id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        app.update();
        let status = app.world().get::<PointcloudStatus>(entity).unwrap();
        if status.phase == PointcloudPhase::Failed {
            assert!(
                status
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("fixture metadata failure")
            );
            assert!(app.world().get::<PointCloud3d>(entity).is_none());
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "initial load error was not delivered"
        );
        bevy::tasks::tick_global_task_pools_on_main_thread();
        std::thread::yield_now();
    }
}
