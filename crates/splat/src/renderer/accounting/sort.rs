//! Sample sorting capacities independently of adapter admission reservations.
//! IDs only: observing an allocation must not keep its GPU handle alive.
use super::*;
use crate::diagnostics::{GpuBufferCapacityReport, SortGpuAccountingReport, SortGpuBufferReport};
use bevy_gaussian_splatting::{
    Gaussian3d,
    sort::{GpuSortedEntry, SortedEntries, SortedEntriesHandle, radix::RadixSortBuffers},
};

#[derive(Resource, Clone, Default)]
pub struct SortGpuAccounting(Arc<Mutex<SortGpuAccountingReport>>);

impl SortGpuAccounting {
    /// Read the latest asynchronous sort allocation observations.
    pub fn telemetry(&self) -> SortGpuAccountingReport {
        self.0.lock().map(|report| *report).unwrap_or_else(|_| {
            SortGpuAccountingReport::unavailable("sort accounting lock poisoned")
        })
    }

    #[cfg(test)]
    fn report(&self) -> Option<SortGpuBufferReport> {
        self.telemetry().sort_gpu_buffers
    }
}

/// Install the resources and systems owned by this module.
pub(super) fn install(app: &mut App) {
    let accounting = SortGpuAccounting(Arc::new(Mutex::new(SortGpuAccountingReport::unavailable(
        "awaiting render extraction",
    ))));
    app.insert_resource(accounting.clone());
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(accounting)
            .add_systems(ExtractSchedule, observe_sort_buffers);
    }
}

#[derive(Default)]
struct BufferObservation {
    buffers: HashMap<BufferId, u64>,
    report: GpuBufferCapacityReport,
}

impl BufferObservation {
    fn observe(
        &mut self,
        buffers: HashMap<BufferId, u64>,
        live_assets: usize,
    ) -> GpuBufferCapacityReport {
        for (id, bytes) in &buffers {
            if !self.buffers.contains_key(id) {
                self.report.observed_bytes_total =
                    self.report.observed_bytes_total.saturating_add(*bytes);
                self.report.observed_buffers_total =
                    self.report.observed_buffers_total.saturating_add(1);
            }
        }
        self.report.live_bytes = buffers.values().sum();
        self.report.live_buffers = buffers.len();
        self.report.live_assets = live_assets;
        self.buffers = buffers;
        self.report
    }
}

#[derive(Default)]
struct ObservedSorts {
    clouds: HashSet<AssetId<PlanarGaussian3d>>,
    sorts: HashSet<AssetId<SortedEntries>>,
    entries: BufferObservation,
    scratch: BufferObservation,
}

#[allow(clippy::type_complexity)]
fn observe_sort_buffers(
    snapshots: Extract<
        Query<(&PlanarGaussian3dHandle, Option<&SortedEntriesHandle>), With<Snapshot>>,
    >,
    sorted: Option<Res<RenderAssets<GpuSortedEntry>>>,
    scratch: Option<Res<RadixSortBuffers<Gaussian3d>>>,
    mut observed: Local<ObservedSorts>,
    accounting: Res<SortGpuAccounting>,
) {
    let (Some(sorted), Some(scratch)) = (sorted, scratch) else {
        if let Ok(mut report) = accounting.0.lock() {
            *report = SortGpuAccountingReport::unavailable("render sort resources unavailable");
        }
        return;
    };
    let mut owned_clouds = HashSet::new();
    let mut owned_sorts = HashSet::new();
    for (cloud, entries) in &snapshots {
        owned_clouds.insert(cloud.0.id());
        if let Some(entries) = entries {
            owned_sorts.insert(entries.0.id());
        }
    }
    observed.clouds.extend(owned_clouds.iter().copied());
    observed.sorts.extend(owned_sorts.iter().copied());
    // Pending entities can precede preparation. Retired main-world entities can
    // leave render allocations behind. Track each until its actual owner retires.
    observed
        .clouds
        .retain(|id| owned_clouds.contains(id) || scratch.asset_map.contains_key(id));
    observed
        .sorts
        .retain(|id| owned_sorts.contains(id) || sorted.get(*id).is_some());

    let mut entry_buffers = HashMap::new();
    let mut entry_assets = 0;
    for id in &observed.sorts {
        if let Some(asset) = sorted.get(*id) {
            entry_assets += 1;
            entry_buffers.insert(
                asset.sorted_entry_buffer.id(),
                asset.sorted_entry_buffer.size(),
            );
        }
    }
    let mut scratch_buffers = HashMap::new();
    let mut scratch_assets = 0;
    for id in &observed.clouds {
        let Some(asset) = scratch.asset_map.get(id) else {
            continue;
        };
        scratch_assets += 1;
        for buffer in [
            &asset.sorting_global_buffer,
            &asset.sorting_status_counter_buffer,
            &asset.entry_buffer_b,
            &asset.dispatch_indirect_buffer,
        ]
        .into_iter()
        .chain(asset.sorting_pass_buffers.iter())
        {
            scratch_buffers.insert(buffer.id(), buffer.size());
        }
    }
    let report = SortGpuBufferReport {
        sorted_entries: observed.entries.observe(entry_buffers, entry_assets),
        radix_scratch: observed.scratch.observe(scratch_buffers, scratch_assets),
    };
    if let Ok(mut published) = accounting.0.lock() {
        *published = SortGpuAccountingReport {
            sort_gpu_buffers: Some(report),
            sort_gpu_buffers_unavailable_reason: None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        ecs::system::SystemState,
        render::{
            MainWorld,
            render_asset::RenderAsset,
            renderer::RenderDevice,
            settings::{Backends, WgpuSettings},
        },
    };
    use bevy_gaussian_splatting::sort::radix::GpuRadixBuffers;

    #[test]
    fn absent_sort_resources_are_not_reported_as_zero_allocations() {
        let mut app = App::new();
        app.insert_sub_app(RenderApp, bevy::app::SubApp::new());
        install(&mut app);
        assert!(
            app.world()
                .resource::<SortGpuAccounting>()
                .report()
                .is_none()
        );
        let render = app.get_sub_app_mut(RenderApp).expect("render app");
        render.init_resource::<MainWorld>();
        render.world_mut().run_schedule(ExtractSchedule);
        assert!(
            render
                .world()
                .resource::<SortGpuAccounting>()
                .report()
                .is_none()
        );
        render
            .init_resource::<RenderAssets<GpuSortedEntry>>()
            .init_resource::<RadixSortBuffers<Gaussian3d>>();
        render.world_mut().run_schedule(ExtractSchedule);
        assert_eq!(
            app.world().resource::<SortGpuAccounting>().report(),
            Some(SortGpuBufferReport::default())
        );
    }

    fn prepare(
        app: &mut App,
        id: AssetId<SortedEntries>,
        count: usize,
    ) -> (GpuSortedEntry, GpuRadixBuffers, u64, u64) {
        let mut params = SystemState::<Res<RenderDevice>>::new(app.world_mut());
        let mut device = params.get(app.world()).expect("device");
        let entries =
            GpuSortedEntry::prepare_asset(SortedEntries::new(1, count), id, &mut device, None)
                .expect("prepare entries");
        let scratch = GpuRadixBuffers::new(count, &device);
        let entries_bytes = entries.sorted_entry_buffer.size();
        let scratch_bytes = scratch.sorting_global_buffer.size()
            + scratch.sorting_status_counter_buffer.size()
            + scratch.entry_buffer_b.size()
            + scratch.dispatch_indirect_buffer.size()
            + scratch
                .sorting_pass_buffers
                .iter()
                .map(|buffer| buffer.size())
                .sum::<u64>();
        (entries, scratch, entries_bytes, scratch_bytes)
    }

    #[test]
    #[ignore = "requires native GPU; offscreen allocation correctness only"]
    fn sorting_observations_follow_shared_resized_and_retired_gpu_assets() {
        let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
            Backends::all(),
            None,
            &WgpuSettings::default(),
        ));
        eprintln!("Sort allocation correctness adapter={:?}", resources.2.0);
        let mut app = App::new();
        app.insert_resource(resources.0)
            .insert_sub_app(RenderApp, bevy::app::SubApp::new());
        install(&mut app);
        // Exercise the observer directly with Extract's real MainWorld resource.
        app.init_resource::<MainWorld>()
            .init_resource::<RenderAssets<GpuSortedEntry>>()
            .init_resource::<RadixSortBuffers<Gaussian3d>>()
            .add_systems(Update, observe_sort_buffers);
        let clouds = Assets::<PlanarGaussian3d>::default();
        let sorts = Assets::<SortedEntries>::default();
        let c1 = clouds.reserve_handle();
        let c2 = clouds.reserve_handle();
        let s1 = sorts.reserve_handle();
        let s2 = sorts.reserve_handle();
        let entities = {
            let mut main = app.world_mut().resource_mut::<MainWorld>();
            vec![
                main.spawn((
                    Snapshot,
                    PlanarGaussian3dHandle(c1.clone()),
                    SortedEntriesHandle(s1.clone()),
                ))
                .id(),
                main.spawn((
                    Snapshot,
                    PlanarGaussian3dHandle(c1.clone()),
                    SortedEntriesHandle(s1.clone()),
                ))
                .id(),
                main.spawn((
                    Snapshot,
                    GpuSnapshot,
                    PlanarGaussian3dHandle(c2.clone()),
                    SortedEntriesHandle(s2.clone()),
                ))
                .id(),
            ]
        };
        app.update();
        assert_eq!(
            app.world().resource::<SortGpuAccounting>().report(),
            Some(SortGpuBufferReport::default())
        );
        let (entries, scratch, entry_bytes, scratch_bytes) = prepare(&mut app, s1.id(), 32);
        // Shared physical buffers under two asset IDs still count once.
        for id in [s1.id(), s2.id()] {
            app.world_mut()
                .resource_mut::<RenderAssets<GpuSortedEntry>>()
                .insert(id, entries.clone());
        }
        for id in [c1.id(), c2.id()] {
            app.world_mut()
                .resource_mut::<RadixSortBuffers<Gaussian3d>>()
                .asset_map
                .insert(id, scratch.clone());
        }
        drop((entries, scratch));
        app.update();
        let initial = SortGpuBufferReport {
            sorted_entries: GpuBufferCapacityReport {
                observed_bytes_total: entry_bytes,
                observed_buffers_total: 1,
                live_bytes: entry_bytes,
                live_buffers: 1,
                live_assets: 2,
            },
            radix_scratch: GpuBufferCapacityReport {
                observed_bytes_total: scratch_bytes,
                observed_buffers_total: 8,
                live_bytes: scratch_bytes,
                live_buffers: 8,
                live_assets: 2,
            },
        };
        assert_eq!(
            app.world().resource::<SortGpuAccounting>().report(),
            Some(initial)
        );
        app.update();
        assert_eq!(
            app.world().resource::<SortGpuAccounting>().report(),
            Some(initial)
        );

        let (entries, scratch, new_entry_bytes, new_scratch_bytes) =
            prepare(&mut app, s1.id(), 8192);
        app.world_mut()
            .resource_mut::<RenderAssets<GpuSortedEntry>>()
            .insert(s1.id(), entries);
        app.world_mut()
            .resource_mut::<RadixSortBuffers<Gaussian3d>>()
            .asset_map
            .insert(c1.id(), scratch);
        app.update();
        let overlap = SortGpuBufferReport {
            sorted_entries: GpuBufferCapacityReport {
                observed_bytes_total: entry_bytes + new_entry_bytes,
                observed_buffers_total: 2,
                live_bytes: entry_bytes + new_entry_bytes,
                live_buffers: 2,
                live_assets: 2,
            },
            radix_scratch: GpuBufferCapacityReport {
                observed_bytes_total: scratch_bytes + new_scratch_bytes,
                observed_buffers_total: 16,
                live_bytes: scratch_bytes + new_scratch_bytes,
                live_buffers: 16,
                live_assets: 2,
            },
        };
        assert_eq!(
            app.world().resource::<SortGpuAccounting>().report(),
            Some(overlap)
        );
        // Flat clouds are outside the streamed metric even with prepared buffers.
        let flat_cloud = clouds.reserve_handle();
        let flat_sort = sorts.reserve_handle();
        let (entries, scratch, _, _) = prepare(&mut app, flat_sort.id(), 16);
        app.world_mut().resource_mut::<MainWorld>().spawn((
            PlanarGaussian3dHandle(flat_cloud.clone()),
            SortedEntriesHandle(flat_sort.clone()),
        ));
        app.world_mut()
            .resource_mut::<RenderAssets<GpuSortedEntry>>()
            .insert(flat_sort.id(), entries);
        app.world_mut()
            .resource_mut::<RadixSortBuffers<Gaussian3d>>()
            .asset_map
            .insert(flat_cloud.id(), scratch);
        for entity in entities {
            app.world_mut().resource_mut::<MainWorld>().despawn(entity);
        }
        app.update();
        assert_eq!(
            app.world().resource::<SortGpuAccounting>().report(),
            Some(overlap),
            "despawn alone does not prove render allocation retirement"
        );
        for id in [s1.id(), s2.id()] {
            app.world_mut()
                .resource_mut::<RenderAssets<GpuSortedEntry>>()
                .remove(id);
        }
        for id in [c1.id(), c2.id()] {
            app.world_mut()
                .resource_mut::<RadixSortBuffers<Gaussian3d>>()
                .asset_map
                .remove(&id);
        }
        app.update();
        let final_report = app
            .world()
            .resource::<SortGpuAccounting>()
            .report()
            .expect("observed");
        assert_eq!(
            final_report.sorted_entries,
            GpuBufferCapacityReport {
                live_bytes: 0,
                live_buffers: 0,
                live_assets: 0,
                ..overlap.sorted_entries
            }
        );
        assert_eq!(
            final_report.radix_scratch,
            GpuBufferCapacityReport {
                live_bytes: 0,
                live_buffers: 0,
                live_assets: 0,
                ..overlap.radix_scratch
            }
        );
        app.world_mut()
            .remove_resource::<RadixSortBuffers<Gaussian3d>>();
        app.update();
        assert!(
            app.world()
                .resource::<SortGpuAccounting>()
                .report()
                .is_none(),
            "resource loss must not leave stale capacities"
        );
    }
}
