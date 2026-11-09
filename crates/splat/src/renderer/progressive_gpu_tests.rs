use super::*;
use bevy::render::{
    renderer::RenderQueue,
    settings::{Backends, WgpuSettings},
};
use bevy_gaussian_splatting::Gaussian3d;

fn readback(device: &RenderDevice, queue: &RenderQueue, buffer: &Buffer) -> Vec<u8> {
    let read = device.create_buffer(&BufferDescriptor {
        label: Some("progressive PLY readback"),
        size: buffer.size(),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(buffer, 0, &read, 0, buffer.size());
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(MapMode::Read, move |result| send.send(result).unwrap());
    device.poll(PollType::wait_indefinitely()).unwrap();
    receive.recv().unwrap().unwrap();
    let bytes = read.slice(..).get_mapped_range().to_vec();
    read.unmap();
    bytes
}

fn batch(start: usize, count: usize, total: usize) -> crate::progressive::decoder::Batch {
    crate::progressive::decoder::Batch {
        cloud: (start..start + count)
            .map(|i| {
                let mut splat = Gaussian3d {
                    position_visibility: [i as f32, 2., 3., 1.].into(),
                    scale_opacity: [0.1, 0.2, 0.3, 0.75].into(),
                    rotation: [1., 0.1, 0.2, 0.3].into(),
                    ..default()
                };
                splat.spherical_harmonic.coefficients[0] = i as f32 + 0.5;
                splat
            })
            .collect::<Vec<_>>()
            .into(),
        count,
        total,
        bounds: (
            Vec3::new(start as f32, 2., 3.),
            Vec3::new((start + count - 1) as f32, 2., 3.),
        ),
        support: 0.9,
    }
}

#[test]
fn progressive_capacity_checks_all_bindings_and_scratch() {
    let defs = bevy_gaussian_splatting::render::ShaderDefines::default();
    assert!(checked_capacity(0, u64::MAX, u64::MAX).is_err());
    assert!(checked_capacity(usize::MAX, u64::MAX, u64::MAX).is_err());
    assert!(checked_capacity(1, u64::from(defs.sorting_buffer_size) - 1, u64::MAX).is_err());
    assert!(checked_capacity(1, u64::MAX, 15).is_err());
    assert_eq!(checked_capacity(33, u64::MAX, u64::MAX).unwrap(), 64);
}

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn progressive_gpu_appends_only_new_rows_and_cancels_pending_writes() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings::default(),
    ));
    eprintln!("progressive PLY GPU adapter={:?}", resources.2.0);
    let (device, queue) = (resources.0, resources.1);
    let mut world = World::new();
    world.insert_resource(device.clone());
    world.init_resource::<Assets<PlanarGaussian3d>>();
    world.init_resource::<Assets<SortedEntries>>();
    world.init_resource::<GpuTransfers>();
    world.insert_resource(GpuBudget {
        selected_bytes_per_scene: 0,
        ..default()
    });
    assert!(ProgressiveGpu::new(&mut world, 35).is_err());
    world.insert_resource(GpuBudget::default());
    let mut gpu = ProgressiveGpu::new(&mut world, 35).unwrap();
    let entity = gpu.entity();
    assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Hidden));
    assert_eq!(world.get::<Transform>(entity), Some(&Transform::default()));
    assert_eq!(
        world.get::<CloudSettings>(entity).unwrap().raster_profile,
        GaussianRasterProfile::NormalizedGaussian
    );
    let mut first = batch(0, 17, 35);
    first.support = 0.3;
    gpu.append(&mut world, &first).unwrap();
    let publication = world
        .resource::<GpuTransfers>()
        .0
        .lock()
        .unwrap()
        .publish
        .remove(0);
    let before = readback(&device, &queue, &publication.cloud.position_visibility);
    assert!(
        before.iter().all(|byte| *byte == 0),
        "append submitted before extraction"
    );
    queue.submit([publication.selection_copies.into_inner().finish()]);
    let first_positions = readback(&device, &queue, &publication.cloud.position_visibility);
    let first_scale = readback(&device, &queue, &publication.cloud.scale_opacity);
    let first_sh = readback(&device, &queue, &publication.cloud.spherical_harmonic);
    let first_rotation = readback(&device, &queue, &publication.cloud.rotation);
    assert_eq!(
        publication.cloud.count, 64,
        "sort uses full declared padded capacity"
    );
    assert!(first_positions[17 * 16..].iter().all(|byte| *byte == 0));
    assert!(first_scale[17 * 16..].iter().all(|byte| *byte == 0));
    assert!(
        first_sh[17 * gpu::strides()[1] as usize..]
            .iter()
            .all(|byte| *byte == 0)
    );
    assert!(first_rotation[17 * 16..].iter().all(|byte| *byte == 0));
    let second = batch(17, 18, 35);
    gpu.append(&mut world, &second).unwrap();
    let next = world
        .resource::<GpuTransfers>()
        .0
        .lock()
        .unwrap()
        .publish
        .remove(0);
    assert_eq!(next.cloud_handle.id(), publication.cloud_handle.id());
    assert_eq!(next.sort_handle.id(), publication.sort_handle.id());
    assert_eq!(next.entity, entity);
    assert_eq!(
        readback(&device, &queue, &next.cloud.position_visibility),
        first_positions
    );
    queue.submit([next.selection_copies.into_inner().finish()]);
    let positions = readback(&device, &queue, &next.cloud.position_visibility);
    let scales = readback(&device, &queue, &next.cloud.scale_opacity);
    let harmonics = readback(&device, &queue, &next.cloud.spherical_harmonic);
    let rotations = readback(&device, &queue, &next.cloud.rotation);
    assert_eq!(&positions[..17 * 16], &first_positions[..17 * 16]);
    assert_eq!(&scales[..17 * 16], &first_scale[..17 * 16]);
    assert_eq!(
        &harmonics[..17 * gpu::strides()[1] as usize],
        &first_sh[..17 * gpu::strides()[1] as usize]
    );
    assert_eq!(&rotations[..17 * 16], &first_rotation[..17 * 16]);
    let sh_floats: &[f32] = bytemuck::cast_slice(&harmonics);
    assert_eq!(sh_floats[34 * gpu::strides()[1] as usize / 4], 34.5);
    let floats: &[f32] = bytemuck::cast_slice(&positions);
    assert_eq!(floats[34 * 4], 34.);
    assert!(positions[35 * 16..].iter().all(|byte| *byte == 0));
    assert!(scales[35 * 16..].iter().all(|byte| *byte == 0));
    assert!(
        harmonics[35 * gpu::strides()[1] as usize..]
            .iter()
            .all(|byte| *byte == 0)
    );
    assert!(rotations[35 * 16..].iter().all(|byte| *byte == 0));
    assert_eq!(
        gpu.uploaded_bytes(),
        35 * gpu::strides().iter().sum::<u64>()
    );
    assert_eq!(
        publication.copy_bytes + next.copy_bytes,
        gpu.uploaded_bytes()
    );
    let bounds = world.get::<Aabb>(entity).unwrap();
    assert!((Vec3::from(bounds.min()) - Vec3::new(-0.9, 1.1, 2.1)).length() < 0.0001);
    assert!((Vec3::from(bounds.max()) - Vec3::new(34.9, 2.9, 3.9)).length() < 0.0001);
    assert!(gpu.append(&mut world, &batch(35, 1, 35)).is_err());
    assert_eq!(world.resource::<Assets<PlanarGaussian3d>>().len(), 0);
    assert_eq!(world.resource::<Assets<SortedEntries>>().len(), 0);
    gpu.retire(&mut world);
    assert!(world.get_entity(entity).is_err());
    assert_eq!(
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .retire
            .len(),
        1
    );

    let mut canceled = ProgressiveGpu::new(&mut world, 35).unwrap();
    canceled.append(&mut world, &first).unwrap();
    let pending_buffer = world.resource::<GpuTransfers>().0.lock().unwrap().publish[0]
        .cloud
        .position_visibility
        .clone();
    canceled.retire(&mut world);
    assert!(
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .publish
            .is_empty()
    );
    assert!(
        readback(&device, &queue, &pending_buffer)
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
#[ignore = "native offscreen GPU parity/readiness; run explicitly with --ignored --nocapture"]
fn progressive_gpu_readiness_and_rendered_pixels_match_complete_planar_cloud() {
    use crate::{
        renderer::readiness::ReadySnapshots,
        streaming::runtime::raster_handoff_tests::{capture, coloured_pixels, render_app, until},
    };

    fn fixture(position: Vec3, scale: Vec3, rgb: Vec3) -> Gaussian3d {
        let mut splat = Gaussian3d {
            position_visibility: [position.x, position.y, position.z, 1.].into(),
            rotation: [1., 0., 0., 0.].into(),
            scale_opacity: [scale.x, scale.y, scale.z, 0.65].into(),
            ..default()
        };
        for (coefficient, color) in splat.spherical_harmonic.coefficients[..3]
            .iter_mut()
            .zip(rgb.to_array())
        {
            *coefficient = (color - 0.5) / 0.282_094_8;
        }
        splat.spherical_harmonic.coefficients[3] = 0.4;
        splat
    }

    fn decoded(row: Gaussian3d) -> crate::progressive::decoder::Batch {
        let center = Vec3::from_array(row.position_visibility.position);
        crate::progressive::decoder::Batch {
            cloud: vec![row].into(),
            count: 1,
            total: 2,
            bounds: (center, center),
            support: Vec3::from_array(row.scale_opacity.scale).max_element() * 3.,
        }
    }

    fn reference(world: &mut World, rows: &[Gaussian3d]) -> Entity {
        let mut padded = vec![Gaussian3d::default(); 32];
        padded[..rows.len()].copy_from_slice(rows);
        let cloud = world.resource_mut::<Assets<PlanarGaussian3d>>().add(padded);
        world
            .spawn((
                Snapshot,
                PlanarGaussian3dHandle(cloud),
                CloudSettings {
                    gaussian_mode: GaussianMode::Gaussian3d,
                    raster_profile: GaussianRasterProfile::NormalizedGaussian,
                    ..default()
                },
                Visibility::Hidden,
                Transform::default(),
                Aabb::from_min_max(Vec3::splat(-4.), Vec3::splat(4.)),
            ))
            .id()
    }

    let (mut app, target, _) = render_app(32);
    // Deliberately append near before far: ordinary GPU radix sorting must
    // restore depth order. Different axes exercise anisotropic covariance, and
    // nonzero higher coefficients exercise SH shading on the same renderer.
    let rows = [
        fixture(
            Vec3::new(-0.12, 0., -2.),
            Vec3::new(0.38, 0.12, 0.2),
            Vec3::X,
        ),
        fixture(
            Vec3::new(0.15, 0.03, -2.2),
            Vec3::new(0.12, 0.3, 0.1),
            Vec3::Y,
        ),
    ];
    let first_reference = reference(app.world_mut(), &rows[..1]);
    until(&mut app, "first ordinary reference ready", |app| {
        app.world()
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .contains(&first_reference)
    });
    app.world_mut()
        .entity_mut(first_reference)
        .insert(Visibility::Visible);
    // Readiness warms radix while the entity is hidden. Draw specialization
    // starts after visibility, so wait for actual reference pixels as well.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let ordinary_first = loop {
        let pixels = capture(&mut app, &target);
        if coloured_pixels(&pixels, 0) > 10 {
            break pixels;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "reference must render visible red pixels"
        );
    };
    app.world_mut()
        .entity_mut(first_reference)
        .insert(Visibility::Hidden);

    let mut progressive = ProgressiveGpu::new(app.world_mut(), 2).unwrap();
    let entity = progressive.entity();
    progressive
        .append(app.world_mut(), &decoded(rows[0]))
        .unwrap();
    until(
        &mut app,
        "first progressive batch reaches real radix readiness",
        |app| {
            app.world()
                .resource::<ReadySnapshots>()
                .0
                .lock()
                .unwrap()
                .contains(&entity)
        },
    );
    assert_eq!(
        app.world().get::<Visibility>(entity),
        Some(&Visibility::Hidden)
    );
    app.world_mut()
        .entity_mut(entity)
        .insert(Visibility::Visible);
    assert_eq!(
        capture(&mut app, &target),
        ordinary_first,
        "unreceived tail changed first-batch pixels"
    );

    progressive
        .append(app.world_mut(), &decoded(rows[1]))
        .unwrap();
    let progressive_full = capture(&mut app, &target);
    assert!(
        coloured_pixels(&progressive_full, 1) > 10,
        "second green batch must render"
    );
    assert_ne!(
        progressive_full, ordinary_first,
        "append must alter actual pixels"
    );
    app.world_mut()
        .entity_mut(entity)
        .insert(Visibility::Hidden);
    let full_reference = reference(app.world_mut(), &rows);
    until(&mut app, "complete ordinary reference ready", |app| {
        app.world()
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .contains(&full_reference)
    });
    app.world_mut()
        .entity_mut(full_reference)
        .insert(Visibility::Visible);
    assert_eq!(
        capture(&mut app, &target),
        progressive_full,
        "appended anisotropic/depth/SH pixels differ from complete cloud"
    );
    app.world_mut()
        .entity_mut(full_reference)
        .insert(Visibility::Hidden);
    app.world_mut()
        .entity_mut(entity)
        .insert(Visibility::Visible);
    let cloud_id = progressive.cloud_handle.id();
    let sort_id = progressive.sort_handle.id();
    progressive.retire(app.world_mut());
    assert!(
        capture(&mut app, &target)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[..3] == [0, 0, 0]),
        "retirement left stale pixels"
    );
    let render = app.sub_app(bevy::render::RenderApp).world();
    assert!(
        render
            .resource::<bevy::render::render_asset::RenderAssets<PlanarStorageGaussian3d>>()
            .get(cloud_id)
            .is_none()
    );
    assert!(
        render
            .resource::<bevy::render::render_asset::RenderAssets<GpuSortedEntry>>()
            .get(sort_id)
            .is_none()
    );
    assert!(
        !render
            .resource::<bevy_gaussian_splatting::sort::radix::RadixSortBuffers<Gaussian3d>>()
            .asset_map
            .contains_key(&cloud_id)
    );
}
