//! Opt-in observation of upstream snapshot buffers. No buffer handles are retained:
//! accounting must never extend the lifetime of the allocations it measures.
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use bevy::{
    prelude::*,
    render::{Extract, RenderApp, render_asset::RenderAssets, render_resource::BufferId},
};
use bevy_gaussian_splatting::{
    PlanarGaussian3d, PlanarGaussian3dHandle, gaussian::formats::planar_3d::PlanarStorageGaussian3d,
};

use super::readiness::{GpuSnapshot, Snapshot};
use crate::diagnostics::SnapshotGpuBufferReport;

mod sort;
pub use sort::SortGpuAccounting;

#[derive(Resource, Clone, Default)]
pub struct SnapshotGpuAccounting(Arc<Mutex<Option<SnapshotGpuBufferReport>>>);

impl SnapshotGpuAccounting {
    /// Return the latest bounded accounting observations.
    pub fn report(&self) -> Option<SnapshotGpuBufferReport> {
        self.0.lock().ok().and_then(|report| *report)
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
    use bevy_gaussian_splatting::Gaussian3d;

    #[test]
    fn installed_bridge_reports_only_after_render_extraction() {
        let mut app = App::new();
        app.insert_sub_app(RenderApp, bevy::app::SubApp::new());
        install(&mut app);
        assert!(
            app.world()
                .resource::<SnapshotGpuAccounting>()
                .report()
                .is_none()
        );
        let render = app.get_sub_app_mut(RenderApp).expect("render app");
        render
            .init_resource::<MainWorld>()
            .init_resource::<RenderAssets<PlanarStorageGaussian3d>>();
        render.world_mut().run_schedule(ExtractSchedule);
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(SnapshotGpuBufferReport::default())
        );
    }

    fn prepare(
        app: &mut App,
        source: PlanarGaussian3d,
        id: AssetId<PlanarGaussian3d>,
    ) -> PlanarStorageGaussian3d {
        let mut params = SystemState::<Res<RenderDevice>>::new(app.world_mut());
        PlanarStorageGaussian3d::prepare_asset(
            source,
            id,
            &mut params.get(app.world()).expect("device"),
            None,
        )
        .expect("prepare snapshot")
    }

    #[test]
    #[ignore = "requires native GPU; offscreen correctness only"]
    fn snapshot_buffer_observations_follow_render_asset_lifetime() {
        let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
            Backends::all(),
            None,
            &WgpuSettings::default(),
        ));
        eprintln!("Snapshot buffer correctness adapter={:?}", resources.2.0);
        let mut app = App::new();
        app.insert_resource(resources.0)
            .init_resource::<MainWorld>()
            .init_resource::<RenderAssets<PlanarStorageGaussian3d>>()
            .init_resource::<ObservedSnapshots>()
            .init_resource::<SnapshotGpuAccounting>()
            .add_systems(Update, observe_snapshot_buffers);
        assert!(
            app.world()
                .resource::<SnapshotGpuAccounting>()
                .report()
                .is_none()
        );
        let mut handles = Assets::<PlanarGaussian3d>::default();
        let handle = handles.add(
            (0..32)
                .map(|_| Gaussian3d::default())
                .collect::<PlanarGaussian3d>(),
        );
        let entity = app
            .world_mut()
            .resource_mut::<MainWorld>()
            .spawn((Snapshot, PlanarGaussian3dHandle(handle.clone())))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(SnapshotGpuBufferReport::default())
        );

        let gpu = prepare(
            &mut app,
            handles.get(&handle).expect("source").clone(),
            handle.id(),
        );
        let bytes: u64 = [
            &gpu.position_visibility,
            &gpu.spherical_harmonic,
            &gpu.rotation,
            &gpu.scale_opacity,
            &gpu.draw_indirect_buffer,
        ]
        .into_iter()
        .map(|buffer| buffer.size())
        .sum();
        assert!(bytes > 16);
        app.world_mut()
            .resource_mut::<RenderAssets<PlanarStorageGaussian3d>>()
            .insert(handle.id(), gpu);
        app.update();
        let expected = SnapshotGpuBufferReport {
            prepared_bytes_total: bytes,
            prepared_buffers_total: 5,
            live_bytes: bytes,
            live_buffers: 5,
            live_assets: 1,
        };
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(expected)
        );
        let duplicate = app
            .world_mut()
            .resource_mut::<MainWorld>()
            .spawn((Snapshot, PlanarGaussian3dHandle(handle.clone())))
            .id();
        app.update();
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(expected)
        );
        // Neither flat clouds nor the persistent-GPU backend belong to this metric.
        let next = handles.add(handles.get(&handle).expect("source").clone());
        let next_gpu = prepare(
            &mut app,
            handles.get(&next).expect("source").clone(),
            next.id(),
        );
        app.world_mut()
            .resource_mut::<RenderAssets<PlanarStorageGaussian3d>>()
            .insert(next.id(), next_gpu);
        let excluded = {
            let mut main = app.world_mut().resource_mut::<MainWorld>();
            main.spawn(PlanarGaussian3dHandle(next.clone()));
            main.spawn((Snapshot, GpuSnapshot, PlanarGaussian3dHandle(next.clone())))
                .id()
        };
        app.update();
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(expected)
        );
        // Admit a real second snapshot generation: both are live during handoff.
        app.world_mut()
            .resource_mut::<MainWorld>()
            .entity_mut(excluded)
            .remove::<GpuSnapshot>();
        app.update();
        let overlap = SnapshotGpuBufferReport {
            prepared_bytes_total: bytes * 2,
            prepared_buffers_total: 10,
            live_bytes: bytes * 2,
            live_buffers: 10,
            live_assets: 2,
        };
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(overlap)
        );
        {
            let mut main = app.world_mut().resource_mut::<MainWorld>();
            main.despawn(entity);
            main.despawn(duplicate);
            main.despawn(excluded);
        }
        app.update();
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(overlap),
            "main-world despawn does not prove GPU assets are released"
        );
        app.world_mut()
            .resource_mut::<RenderAssets<PlanarStorageGaussian3d>>()
            .remove(handle.id());
        app.world_mut()
            .resource_mut::<RenderAssets<PlanarStorageGaussian3d>>()
            .remove(next.id());
        app.update();
        assert_eq!(
            app.world().resource::<SnapshotGpuAccounting>().report(),
            Some(SnapshotGpuBufferReport {
                live_bytes: 0,
                live_buffers: 0,
                live_assets: 0,
                ..overlap
            })
        );
        assert!(
            app.world()
                .resource::<ObservedSnapshots>()
                .assets
                .is_empty()
        );
        assert!(
            app.world()
                .resource::<ObservedSnapshots>()
                .buffers
                .is_empty()
        );
    }
}

#[derive(Resource, Default)]
struct ObservedSnapshots {
    assets: HashSet<AssetId<PlanarGaussian3d>>,
    buffers: HashMap<BufferId, u64>,
    report: SnapshotGpuBufferReport,
}

/// Install the resources and systems owned by this module.
pub fn install(app: &mut App) {
    sort::install(app);
    let accounting = SnapshotGpuAccounting::default();
    app.insert_resource(accounting.clone());
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(accounting)
            .init_resource::<ObservedSnapshots>()
            .add_systems(ExtractSchedule, observe_snapshot_buffers);
    }
}

#[allow(
    clippy::type_complexity,
    reason = "The extract query describes the required snapshot filters."
)]
fn observe_snapshot_buffers(
    snapshots: Extract<Query<&PlanarGaussian3dHandle, (With<Snapshot>, Without<GpuSnapshot>)>>,
    prepared: Res<RenderAssets<PlanarStorageGaussian3d>>,
    mut observed: ResMut<ObservedSnapshots>,
    accounting: Res<SnapshotGpuAccounting>,
) {
    let owned: HashSet<_> = snapshots.iter().map(|handle| handle.0.id()).collect();
    observed.assets.extend(owned.iter().copied());
    // Extraction observes the previous render's prepared assets. Remember pending
    // main-world assets, and keep retired snapshots until the render asset is gone.
    observed
        .assets
        .retain(|id| owned.contains(id) || prepared.get(*id).is_some());
    let mut buffers = HashMap::new();
    let mut live_assets = 0;
    for id in &observed.assets {
        let Some(asset) = prepared.get(*id) else {
            continue;
        };
        live_assets += 1;
        for buffer in [
            &asset.position_visibility,
            &asset.spherical_harmonic,
            &asset.rotation,
            &asset.scale_opacity,
            &asset.draw_indirect_buffer,
        ] {
            buffers.insert(buffer.id(), buffer.size());
        }
    }
    let new_bytes = buffers
        .iter()
        .filter(|(id, _)| !observed.buffers.contains_key(id))
        .map(|(_, size)| *size)
        .sum::<u64>();
    let new_buffers = buffers
        .keys()
        .filter(|id| !observed.buffers.contains_key(id))
        .count() as u64;
    observed.report.prepared_bytes_total = observed
        .report
        .prepared_bytes_total
        .saturating_add(new_bytes);
    observed.report.prepared_buffers_total = observed
        .report
        .prepared_buffers_total
        .saturating_add(new_buffers);
    observed.report.live_bytes = buffers.values().sum();
    observed.report.live_buffers = buffers.len();
    observed.report.live_assets = live_assets;
    observed.buffers = buffers;
    if let Ok(mut report) = accounting.0.lock() {
        *report = Some(observed.report);
    }
}
