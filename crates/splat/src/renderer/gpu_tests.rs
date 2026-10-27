use super::*;
use crate::renderer::{SnapshotRenderer, SnapshotState, readiness::ReadySnapshots};
use bevy::{
    ecs::system::SystemState,
    render::renderer::RenderQueue,
    render::settings::{Backends, WgpuSettings},
};

fn readback(device: &RenderDevice, queue: &RenderQueue, buffer: &Buffer) -> Vec<u8> {
    let read = device.create_buffer(&BufferDescriptor {
        label: Some("SOG test readback"),
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

#[test]
#[ignore = "requires native GPU; run explicitly with --ignored --nocapture"]
fn resident_gpu_copies_reuse_sources_slots_and_retire_scene() {
    let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
        Backends::all(),
        None,
        &WgpuSettings::default(),
    ));
    eprintln!("SOG adapter correctness/timing adapter={:?}", resources.2.0);
    let device = resources.0;
    let queue = resources.1;
    let mut world = World::new();
    world.insert_resource(crate::diagnostics::sog_cpu::SogCpuTimings::enabled());
    world.init_resource::<Assets<PlanarGaussian3d>>();
    world.init_resource::<Assets<SortedEntries>>();
    world.init_resource::<ReadySnapshots>();
    world.init_resource::<GpuResident>();
    world.init_resource::<GpuTransfers>();
    world.init_resource::<GpuBudget>();
    world.init_resource::<crate::runtime::SogRuntimeSettings>();
    world.init_resource::<crate::runtime::SogSelectionStatistics>();
    world.insert_resource(SogRendererBackend::GpuResident);
    world.insert_resource(device.clone());
    world.insert_resource(queue.clone());
    let scene = world.spawn_empty().id();
    let camera = world
        .spawn((
            Camera::default(),
            bevy::core_pipeline::tonemapping::Tonemapping::Reinhard,
        ))
        .id();
    let mut renderer = SystemState::<SnapshotRenderer>::new(&mut world);
    let mut state = SnapshotState::default();
    let chunk = DecodedChunk {
        cloud: (0..100)
            .map(|x| Gaussian3d {
                position_visibility: [x as f32, 2., 3., 1.].into(),
                scale_opacity: [0.1, 0.2, 0.3, 1.].into(),
                ..default()
            })
            .collect::<Vec<_>>()
            .into(),
        count: 100,
    };
    let total_bytes = 100 * strides().iter().sum::<u64>();
    for (step, (offset, count)) in [(5, 65), (3, 33), (40, 3), (41, 3), (0, 100)]
        .into_iter()
        .enumerate()
    {
        let compaction = step % 2 != 0;
        world
            .resource_mut::<crate::runtime::SogRuntimeSettings>()
            .compaction = compaction;
        let range = [SplatRange {
            file: 0,
            offset,
            count,
        }];
        renderer
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &range, |_| Some(&chunk))
            .unwrap();
        renderer.apply(&mut world);
        assert_eq!(
            world.get::<bevy::core_pipeline::tonemapping::Tonemapping>(camera),
            Some(&bevy::core_pipeline::tonemapping::Tonemapping::Reinhard),
            "GPU submissions must preserve explicit camera appearance"
        );
        let settings = world
            .get::<bevy_gaussian_splatting::CloudSettings>(state.pending.unwrap())
            .unwrap();
        assert_eq!(
            settings.raster_profile,
            bevy_gaussian_splatting::GaussianRasterProfile::NormalizedGaussian
        );
        assert_eq!(settings.visibility_compaction, compaction);
        assert!(!renderer.get_mut(&mut world).unwrap().poll(&mut state));
        let publication = world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .publish
            .pop()
            .unwrap();
        // Simulate the last old render frame reaching the queue after main-world
        // activation/reuse but before the next synchronized extraction. Its
        // read must still observe A's contents, never the pending C selection.
        let before = readback(&device, &queue, &publication.cloud.position_visibility);
        let previous_first = [0., 0., 5., 3., 0.][step];
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&before)[0],
            previous_first,
            "main-world submission overwrote a slot before render extraction"
        );
        // This is the extraction/publication boundary, after that old-frame
        // read has reached the queue. Publishing the generation follows copies.
        queue.submit([publication.selection_copies.into_inner().finish()]);
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .submitted_copy_bytes += publication.copy_bytes;
        let positions = readback(&device, &queue, &publication.cloud.position_visibility);
        let floats: &[f32] = bytemuck::cast_slice(&positions);
        assert_eq!(floats[0], offset as f32);
        assert_eq!(floats[(count - 1) * 4], (offset + count - 1) as f32);
        assert_eq!(publication.cloud.count, count.div_ceil(32) * 32);
        let opacity = readback(&device, &queue, &publication.cloud.scale_opacity);
        let values: &[f32] = bytemuck::cast_slice(&opacity);
        assert!(
            values[count * 4..publication.cloud.count * 4]
                .iter()
                .all(|v| *v == 0.)
        );
        let stats = world.resource::<GpuResident>().statistics;
        let gpu = world.resource::<GpuResident>();
        let memory = gpu.scene_memory(scene);
        assert_eq!(
            gpu.scene_source_bytes(scene),
            total_bytes,
            "shared ranges upload and count one immutable file"
        );
        assert_eq!(memory.selected_gpu_bytes, stats.selected_buffer_bytes);
        assert_eq!(
            memory.radix_reserved_bytes,
            stats.radix_scratch_bytes_reserved
        );
        assert_eq!(
            memory.cpu_snapshot_bytes, 0,
            "resident slots are not CPU snapshots"
        );
        assert_eq!(memory.generations, (step + 1).min(2));
        assert_eq!(
            stats.uploaded_bytes, total_bytes,
            "resident source uploaded again"
        );
        assert_eq!(
            stats.allocations,
            if step == 4 {
                3
            } else {
                ((step + 1).min(2)) as u64
            }
        );
        assert_eq!(world.resource::<Assets<PlanarGaussian3d>>().len(), 0);
        assert_eq!(world.resource::<Assets<SortedEntries>>().len(), 0);
        world
            .resource::<ReadySnapshots>()
            .0
            .lock()
            .unwrap()
            .insert(publication.entity);
        assert!(renderer.get_mut(&mut world).unwrap().poll(&mut state));
        renderer.apply(&mut world);
        assert_eq!(
            world.resource::<GpuResident>().scene_memory(scene),
            memory,
            "handoff releases pins, not reusable high-water slot allocations"
        );
        let settings = world
            .get::<bevy_gaussian_splatting::CloudSettings>(state.current.unwrap())
            .unwrap();
        assert_eq!(
            settings.raster_profile,
            bevy_gaussian_splatting::GaussianRasterProfile::NormalizedGaussian
        );
        assert_eq!(settings.visibility_compaction, compaction);
        let previous = state.current;
        let bad = [SplatRange {
            file: 0,
            offset: 100,
            count: 1,
        }];
        assert!(
            renderer
                .get_mut(&mut world)
                .unwrap()
                .submit(&mut state, scene, camera, &bad, |_| Some(&chunk))
                .is_err()
        );
        assert_eq!(state.current, previous);
        assert!(!state.is_pending());
    }
    let stats = world.resource::<GpuResident>().statistics;
    assert_eq!(stats.reuses, 2);
    assert_eq!(
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .submitted_copy_bytes,
        stats.encoded_copy_bytes
    );
    assert_eq!(
        stats.encoded_copy_bytes,
        (65 + 33 + 3 + 3 + 100) * strides().iter().sum::<u64>()
    );
    renderer.get_mut(&mut world).unwrap().clear(&mut state);
    renderer.apply(&mut world);
    let stats = world.resource::<GpuResident>().statistics;
    assert_eq!(stats.resident_bytes, 0);
    assert_eq!(stats.selected_buffer_bytes, 0);
    assert_eq!(stats.radix_scratch_bytes_reserved, 0);
    assert_eq!(stats.selected_count, 0);
    let cpu = world
        .resource::<crate::diagnostics::sog_cpu::SogCpuTimings>()
        .report()
        .cpu_stages
        .expect("enabled CPU timing");
    assert_eq!(
        cpu.selection_plan.attempts, 10,
        "includes five rejected ranges"
    );
    assert_eq!(
        cpu.source_upload_prepare.attempts, 1,
        "shared source uploads once"
    );
    assert_eq!(
        cpu.selection_copy_prepare.attempts, 5,
        "each encoded generation"
    );
    assert!(
        cpu.snapshot_compose.total_ms.is_none(),
        "GPU path does not compose CPU snapshots"
    );
    assert!(
        cpu.source_upload_prepare.total_ms.is_some(),
        "CPU prepare interval, not GPU time"
    );
    assert_eq!(world.resource::<GpuResident>().scene_source_bytes(scene), 0);
    assert_eq!(
        world.resource::<GpuResident>().scene_memory(scene),
        super::super::SelectionMemory::default()
    );
    assert_eq!(
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .retire
            .len(),
        2
    );
    // Equal file indices in distinct scene entity generations must never alias.
    let scene2 = world.spawn_empty().id();
    let mut state2 = SnapshotState::default();
    let chunk2 = DecodedChunk {
        cloud: (0..100)
            .map(|x| Gaussian3d {
                position_visibility: [1000. + x as f32, 0., 0., 1.].into(),
                ..default()
            })
            .collect::<Vec<_>>()
            .into(),
        count: 100,
    };
    let ranges = [SplatRange {
        file: 0,
        offset: 0,
        count: 1,
    }];
    renderer
        .get_mut(&mut world)
        .unwrap()
        .submit(&mut state, scene, camera, &ranges, |_| Some(&chunk))
        .unwrap();
    renderer.apply(&mut world);
    let entity = state.pending.unwrap();
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(entity);
    assert!(renderer.get_mut(&mut world).unwrap().poll(&mut state));
    renderer.apply(&mut world);
    renderer
        .get_mut(&mut world)
        .unwrap()
        .submit(&mut state2, scene2, camera, &ranges, |_| Some(&chunk2))
        .unwrap();
    renderer.apply(&mut world);
    let second = world
        .resource::<GpuTransfers>()
        .0
        .lock()
        .unwrap()
        .publish
        .pop()
        .unwrap();
    queue.submit([second.selection_copies.into_inner().finish()]);
    world
        .resource::<GpuTransfers>()
        .0
        .lock()
        .unwrap()
        .submitted_copy_bytes += second.copy_bytes;
    let bytes = readback(&device, &queue, &second.cloud.position_visibility);
    assert_eq!(bytemuck::cast_slice::<u8, f32>(&bytes)[0], 1000.);
    assert_eq!(
        world.resource::<GpuResident>().statistics.resident_bytes,
        total_bytes * 2
    );
    assert_eq!(
        world.resource::<GpuResident>().scene_source_bytes(scene),
        total_bytes
    );
    assert_eq!(
        world.resource::<GpuResident>().scene_source_bytes(scene2),
        total_bytes
    );

    // Full residency with active/pending sources pinned must reject atomically.
    world.resource_mut::<GpuBudget>().source_bytes = total_bytes * 2;
    assert_eq!(
        world
            .resource::<GpuResident>()
            .source_headroom(scene, total_bytes * 2),
        total_bytes
    );
    assert_eq!(
        world
            .resource::<GpuResident>()
            .source_headroom(scene2, total_bytes * 2),
        total_bytes
    );
    let next = [SplatRange {
        file: 1,
        offset: 0,
        count: 1,
    }];
    let uploads = world.resource::<GpuResident>().statistics.uploaded_bytes;
    assert!(
        renderer
            .get_mut(&mut world)
            .unwrap()
            .submit(&mut state, scene, camera, &next, |_| Some(&chunk))
            .is_err()
    );
    assert_eq!(
        world.resource::<GpuResident>().statistics.uploaded_bytes,
        uploads
    );
    assert!(!state.is_pending());
    // Removing a scene while its selection is pending retires it automatically.
    world.despawn(scene2);
    use bevy::ecs::system::RunSystemOnce;
    world.run_system_once(cleanup).unwrap();
    assert_eq!(
        world.resource::<GpuResident>().scene_source_bytes(scene2),
        0
    );
    assert_eq!(
        world.resource::<GpuResident>().scene_memory(scene2),
        super::super::SelectionMemory::default()
    );
    assert_eq!(
        world
            .resource::<GpuResident>()
            .source_headroom(scene, total_bytes * 2),
        total_bytes * 2
    );
    assert_eq!(
        world.resource::<GpuResident>().statistics.resident_bytes,
        total_bytes
    );
    renderer
        .get_mut(&mut world)
        .unwrap()
        .submit(&mut state, scene, camera, &next, |_| Some(&chunk))
        .unwrap();
    renderer.apply(&mut world);
    world
        .resource::<ReadySnapshots>()
        .0
        .lock()
        .unwrap()
        .insert(state.pending.unwrap());
    assert!(renderer.get_mut(&mut world).unwrap().poll(&mut state));
    renderer.apply(&mut world);
    let next = [SplatRange {
        file: 2,
        offset: 50,
        count: 1,
    }];
    renderer
        .get_mut(&mut world)
        .unwrap()
        .submit(&mut state, scene, camera, &next, |_| Some(&chunk))
        .unwrap();
    renderer.apply(&mut world);
    assert!(
        !world
            .resource::<GpuResident>()
            .sources
            .contains_key(&(scene, 0)),
        "inactive source was not evicted"
    );
    assert!(
        world
            .resource::<GpuResident>()
            .sources
            .contains_key(&(scene, 1)),
        "current source was evicted"
    );
    assert_eq!(
        world.resource::<GpuResident>().statistics.resident_bytes,
        total_bytes * 2
    );
    assert_eq!(
        world.resource::<GpuResident>().scene_source_bytes(scene),
        total_bytes * 2,
        "eviction drops obsolete source allocation before counting replacement"
    );
    let submitted_before_cancel = world
        .resource::<GpuTransfers>()
        .0
        .lock()
        .unwrap()
        .submitted_copy_bytes;
    let canceled_destination = world.resource::<GpuResident>().scenes[&scene].slots[0]
        .as_ref()
        .unwrap()
        .cloud
        .position_visibility
        .clone();
    // Clear cancels unextracted generations too, so they cannot resurrect assets
    // or submit their pending writes to a retained old-slot buffer.
    renderer.get_mut(&mut world).unwrap().clear(&mut state);
    renderer.apply(&mut world);
    assert!(
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .publish
            .is_empty()
    );
    assert_eq!(world.resource::<GpuResident>().statistics.resident_bytes, 0);
    assert_eq!(
        world.resource::<GpuResident>().scene_memory(scene),
        super::super::SelectionMemory::default()
    );
    assert_eq!(world.resource::<GpuResident>().scene_source_bytes(scene), 0);
    assert_eq!(
        world
            .resource::<GpuTransfers>()
            .0
            .lock()
            .unwrap()
            .submitted_copy_bytes,
        submitted_before_cancel
    );
    let canceled_bytes = readback(&device, &queue, &canceled_destination);
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&canceled_bytes)[0],
        0.,
        "canceled selection was submitted to the queue"
    );
}
