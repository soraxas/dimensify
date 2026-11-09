//! Lifecycle acceptance using real HTTP and native GPU allocation. Readiness is
//! acknowledged explicitly; these tests do not claim extraction or actual draw.
use super::*;
use crate::renderer::gpu::GpuTransfers;
use bevy::render::{
    renderer::{RenderDevice, RenderQueue, initialize_renderer},
    settings::{Backends, WgpuSettings},
};
use bevy_gaussian_splatting::{
    Gaussian3d, PlanarGaussian3d, PlanarGaussian3dHandle, sort::SortedEntries,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Hold the last declared row until released. An empty tail closes a valid HTTP
/// response whose binary PLY payload is truncated.
struct SlowPly {
    url: String,
    release: Option<mpsc::Sender<Vec<u8>>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl SlowPly {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let (release, wait) = mpsc::channel::<Vec<u8>>();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + TIMEOUT;
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "HTTP client did not connect");
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("accepting PLY request: {error}"),
                }
            };
            socket.set_read_timeout(Some(TIMEOUT)).unwrap();
            socket.set_write_timeout(Some(TIMEOUT)).unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).unwrap() > 0);
            // Close-delimited HTTP keeps EOF behind the release signal even
            // when fewer rows than the PLY declared are delivered.
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                .unwrap();
            socket.write_all(&header()).unwrap();
            socket.write_all(&row(0.)).unwrap();
            socket.flush().unwrap();
            if let Ok(tail) = wait.recv_timeout(TIMEOUT + TIMEOUT) {
                // Supersession can close the client before the server resumes.
                let _ = socket.write_all(&tail);
            }
        });
        Self {
            url: format!("http://{address}/scene.ply"),
            release: Some(release),
            worker: Some(worker),
        }
    }

    fn finish(&mut self, tail: Vec<u8>) {
        self.release.take().unwrap().send(tail).unwrap();
        self.worker.take().unwrap().join().unwrap();
    }
}

impl Drop for SlowPly {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(Vec::new());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn header() -> Vec<u8> {
    let mut header = "ply\nformat binary_little_endian 1.0\nelement vertex 2\n".to_owned();
    for property in [
        "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "scale_0", "scale_1", "scale_2", "opacity",
        "rot_0", "rot_1", "rot_2", "rot_3",
    ] {
        header.push_str(&format!("property float {property}\n"));
    }
    header.push_str("end_header\n");
    header.into_bytes()
}

fn row(x: f32) -> Vec<u8> {
    [x, 1., -3., 0.1, 0.2, 0.3, -2., -2., -2., 0., 1., 0., 0., 0.]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect()
}

fn viewer() -> (World, LoadedSplat, AssetId<PlanarGaussian3d>) {
    static GPU: OnceLock<(RenderDevice, RenderQueue)> = OnceLock::new();
    let (device, queue) = GPU.get_or_init(|| {
        let resources = bevy::tasks::block_on(initialize_renderer(
            Backends::all(),
            None,
            &WgpuSettings::default(),
        ));
        eprintln!(
            "progressive viewer lifecycle GPU adapter={:?}",
            resources.2.0
        );
        (resources.0, resources.1)
    });
    let mut world = World::new();
    world.insert_resource(device.clone());
    world.insert_resource(queue.clone());
    world.init_resource::<Assets<PlanarGaussian3d>>();
    world.init_resource::<Assets<SortedEntries>>();
    world.init_resource::<GpuTransfers>();
    world.init_resource::<ReadySnapshots>();
    world.init_resource::<ProgressivePly>();
    world.init_resource::<LocalFiles>();
    world.insert_resource(ViewerOptions {
        up: crate::SourceUp::Y,
        progressive_ply: true,
        ..default()
    });
    world.init_resource::<super::super::streaming::ViewerStreams>();
    world.init_resource::<super::super::settings::ViewerSettings>();
    world.init_resource::<crate::scene::PendingLoad>();
    world.init_resource::<Messages<crate::LoadSplat>>();
    world.init_resource::<Messages<SplatLoaded>>();
    let handle = world
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .add(PlanarGaussian3d::from(vec![Gaussian3d::default()]));
    let asset = handle.id();
    let previous = LoadedSplat {
        entity: world
            .spawn((PlanarGaussian3dHandle(handle), Visibility::Inherited))
            .id(),
        bounds: (Vec3::splat(-7.), Vec3::splat(9.)),
    };
    world.init_resource::<SplatScene>();
    {
        let mut scene = world.resource_mut::<SplatScene>();
        scene.loaded = Some(previous.clone());
        scene.phase = LoadPhase::Ready {
            name: "previous.ply".into(),
            count: 1,
        };
    }
    (world, previous, asset)
}

fn wait_until(world: &mut World, description: &str, ready: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        poll(world);
        if ready(world) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out {description}; scene phase={:?}",
            world.resource::<SplatScene>().phase
        );
        thread::sleep(Duration::from_millis(2));
    }
}

fn partial(world: &mut World, server: &SlowPly) -> Entity {
    start(world, 0, "scene.ply".into(), server.url.clone());
    wait_until(
        world,
        "waiting for the first row before HTTP EOF",
        |world| {
            world
                .resource::<ProgressivePly>()
                .active
                .as_ref()
                .is_some_and(|active| active.count == 1 && active.total == 2 && !active.eof)
        },
    );
    world
        .resource::<ProgressivePly>()
        .active
        .as_ref()
        .unwrap()
        .gpu
        .as_ref()
        .unwrap()
        .entity()
}

fn acknowledge(world: &mut World, candidate: Entity) {
    // This substitutes only for the renderer's later-frame acknowledgment.
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(candidate);
    poll(world);
    assert!(
        world
            .resource::<ProgressivePly>()
            .active
            .as_ref()
            .unwrap()
            .shown
    );
    assert_eq!(loaded(world).entity, candidate);
    assert_eq!(
        world.get::<Visibility>(candidate),
        Some(&Visibility::Inherited)
    );
}

fn loaded(world: &World) -> &LoadedSplat {
    world.resource::<SplatScene>().loaded.as_ref().unwrap()
}

fn supersede_selection(world: &mut World) {
    // Queue a real newer selection without starting its transport, allowing
    // control over its load/commit boundary before the old progressive poll.
    let mut files = world.resource_mut::<LocalFiles>();
    files.progressive_enabled = true;
    files.open_url("http://127.0.0.1:9/newer.ply", crate::SourceUp::Y);
    assert_eq!(files.selection(), 1);
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn partial_http_waits_for_readiness_then_completion_retires_previous_scene() {
    let (mut world, previous, previous_asset) = viewer();
    let mut server = SlowPly::start();
    let candidate = partial(&mut world, &server);
    assert_eq!(loaded(&world).entity, previous.entity);
    assert_eq!(
        world.get::<Visibility>(previous.entity),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        world.get::<Visibility>(candidate),
        Some(&Visibility::Hidden)
    );
    for _ in 0..3 {
        poll(&mut world);
    }
    assert_eq!(
        loaded(&world).entity,
        previous.entity,
        "readiness gates activation"
    );
    assert_eq!(world.resource::<Messages<SplatLoaded>>().len(), 0);
    acknowledge(&mut world, candidate);
    assert_eq!(
        world.get::<Visibility>(previous.entity),
        Some(&Visibility::Hidden)
    );
    assert!(
        world
            .resource::<Assets<PlanarGaussian3d>>()
            .contains(previous_asset)
    );
    let active = world.resource::<ProgressivePly>().active.as_ref().unwrap();
    assert!(
        !active.eof && !active.complete,
        "activation happened before HTTP EOF"
    );
    assert!(
        matches!(&world.resource::<SplatScene>().phase, LoadPhase::Loading(name) if name == "scene.ply (1/2)")
    );
    assert_eq!(world.resource::<Messages<SplatLoaded>>().len(), 1);

    server.finish(row(2.));
    wait_until(
        &mut world,
        "completing the binary PLY",
        |world| matches!(&world.resource::<SplatScene>().phase, LoadPhase::Ready { name, count: 2 } if name == "scene.ply"),
    );
    assert_eq!(loaded(&world).entity, candidate);
    assert!(world.get_entity(previous.entity).is_err());
    assert!(
        !world
            .resource::<Assets<PlanarGaussian3d>>()
            .contains(previous_asset)
    );
    assert!(
        world
            .resource::<ProgressivePly>()
            .active
            .as_ref()
            .unwrap()
            .complete
    );
    poll(&mut world);
    assert_eq!(loaded(&world).entity, candidate);
    assert_eq!(world.resource::<Messages<SplatLoaded>>().len(), 1);
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn late_truncated_http_rolls_back_the_visible_partial_and_preserves_previous_asset() {
    let (mut world, previous, previous_asset) = viewer();
    let mut server = SlowPly::start();
    let candidate = partial(&mut world, &server);
    acknowledge(&mut world, candidate);
    assert_eq!(
        world.get::<Visibility>(previous.entity),
        Some(&Visibility::Hidden)
    );
    server.finish(row(2.)[..7].to_vec());
    wait_until(&mut world, "rolling back a truncated final row", |world| {
        matches!(world.resource::<SplatScene>().phase, LoadPhase::Failed(_))
    });
    assert!(
        matches!(&world.resource::<SplatScene>().phase, LoadPhase::Failed(message) if message.contains("Truncated Gaussian PLY payload"))
    );
    assert_eq!(loaded(&world).entity, previous.entity);
    assert_eq!(loaded(&world).bounds, previous.bounds);
    assert_eq!(
        world.get::<Visibility>(previous.entity),
        Some(&Visibility::Inherited)
    );
    assert!(
        world
            .resource::<Assets<PlanarGaussian3d>>()
            .contains(previous_asset)
    );
    assert!(world.get_entity(candidate).is_err());
    assert!(
        !world
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .contains(&candidate)
    );
    assert!(world.resource::<ProgressivePly>().active.is_none());
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn late_failure_restores_previous_scene_in_the_current_coordinate_basis() {
    let (mut world, previous, previous_asset) = viewer();
    let original_transform = crate::SourceUp::Z.transform();
    let geometry = SourceGeometry {
        bounds: (Vec3::new(-1., 2., 3.), Vec3::new(4., 5., 9.)),
        support: 0.25,
    };
    let original_bounds = geometry.transformed_bounds(original_transform);
    let current_bounds = (Vec3::new(-1.25, 1.75, 2.75), Vec3::new(4.25, 5.25, 9.25));
    assert_ne!(original_bounds, current_bounds);
    world
        .entity_mut(previous.entity)
        .insert((geometry, original_transform));
    world
        .resource_mut::<SplatScene>()
        .loaded
        .as_mut()
        .unwrap()
        .bounds = original_bounds;
    world.resource_mut::<ViewerOptions>().up = crate::SourceUp::Z;

    let mut server = SlowPly::start();
    let candidate = partial(&mut world, &server);
    acknowledge(&mut world, candidate);
    assert_eq!(
        world.get::<Transform>(previous.entity),
        Some(&original_transform)
    );
    // Changing the viewer setting updates the active partial. Its hidden
    // rollback copy must adopt the same current setting when restored.
    world.resource_mut::<ViewerOptions>().up = crate::SourceUp::Y;
    poll(&mut world);
    assert_eq!(
        world.get::<Transform>(candidate),
        Some(&crate::SourceUp::Y.transform())
    );

    server.finish(row(2.)[..7].to_vec());
    wait_until(
        &mut world,
        "restoring the previous scene after its basis changed",
        |world| matches!(world.resource::<SplatScene>().phase, LoadPhase::Failed(_)),
    );
    assert_eq!(loaded(&world).entity, previous.entity);
    assert_eq!(
        world.get::<Transform>(previous.entity),
        Some(&crate::SourceUp::Y.transform()),
        "rollback retained the hidden scene's obsolete coordinate basis"
    );
    assert_eq!(
        loaded(&world).bounds,
        current_bounds,
        "rollback retained bounds from the obsolete coordinate basis"
    );
    assert_eq!(
        world.get::<Visibility>(previous.entity),
        Some(&Visibility::Inherited)
    );
    assert!(
        world
            .resource::<Assets<PlanarGaussian3d>>()
            .contains(previous_asset)
    );
    assert!(world.get_entity(candidate).is_err());
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn superseded_partial_restores_previous_without_overwriting_new_loading_phase() {
    let (mut world, previous, _) = viewer();
    let mut server = SlowPly::start();
    let candidate = partial(&mut world, &server);
    acknowledge(&mut world, candidate);
    supersede_selection(&mut world);
    world.resource_mut::<SplatScene>().phase = LoadPhase::Loading("newer.ply".into());
    poll(&mut world);
    assert_eq!(loaded(&world).entity, previous.entity);
    assert_eq!(
        world.get::<Visibility>(previous.entity),
        Some(&Visibility::Inherited)
    );
    assert!(world.get_entity(candidate).is_err());
    assert!(world.resource::<ProgressivePly>().active.is_none());
    // A late failure from the canceled download cannot alter the new phase.
    server.finish(Vec::new());
    for _ in 0..3 {
        poll(&mut world);
    }
    assert!(
        matches!(&world.resource::<SplatScene>().phase, LoadPhase::Loading(name) if name == "newer.ply")
    );
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn superseded_partial_cannot_roll_back_a_newer_committed_scene() {
    let (mut world, previous, previous_asset) = viewer();
    let mut server = SlowPly::start();
    let candidate = partial(&mut world, &server);
    acknowledge(&mut world, candidate);
    supersede_selection(&mut world);
    // Model a newer whole-file/stream commit earlier in the same frame.
    let newer = world.spawn(Visibility::Inherited).id();
    world.resource_mut::<SplatScene>().loaded = Some(LoadedSplat {
        entity: newer,
        bounds: (Vec3::ZERO, Vec3::ONE),
    });
    world.resource_mut::<SplatScene>().phase = LoadPhase::Ready {
        name: "newer.ply".into(),
        count: 17,
    };
    poll(&mut world);
    assert_eq!(
        loaded(&world).entity,
        newer,
        "stale rollback restored {:?} over the newer scene",
        previous.entity
    );
    assert!(world.get_entity(candidate).is_err());
    assert!(world.get_entity(previous.entity).is_err());
    assert!(
        !world
            .resource::<Assets<PlanarGaussian3d>>()
            .contains(previous_asset)
    );
    assert!(
        matches!(&world.resource::<SplatScene>().phase, LoadPhase::Ready { name, count: 17 } if name == "newer.ply")
    );
    server.finish(Vec::new());
    poll(&mut world);
    assert_eq!(loaded(&world).entity, newer);
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn invalid_new_selection_restores_previous_phase_instead_of_leaving_stale_loading() {
    let (mut world, previous, _) = viewer();
    let mut server = SlowPly::start();
    let candidate = partial(&mut world, &server);
    acknowledge(&mut world, candidate);
    world
        .resource_mut::<LocalFiles>()
        .open_url("not a valid URL", crate::SourceUp::Y);
    assert_eq!(world.resource::<LocalFiles>().selection(), 1);
    assert!(world.resource::<LocalFiles>().notice.is_some());
    poll(&mut world);
    assert_eq!(loaded(&world).entity, previous.entity);
    assert!(world.get_entity(candidate).is_err());
    assert!(
        matches!(&world.resource::<SplatScene>().phase, LoadPhase::Ready { name, count: 1 } if name == "previous.ply"),
        "canceled selection left stale phase {:?}",
        world.resource::<SplatScene>().phase
    );
    server.finish(Vec::new());
}
