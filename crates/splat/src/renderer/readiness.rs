//! Atomic activation: publish GPU assets before extraction, acknowledge only
//! after radix scratch and its pipeline variant are usable in a later frame.
use super::gpu::{GpuBudget, GpuResident, GpuTransfers, SogRendererBackend};
use bevy::{
    prelude::*,
    render::{
        Extract, RenderApp, render_asset::RenderAssets, render_resource::PipelineCache,
        renderer::RenderQueue,
    },
};
use bevy_gaussian_splatting::{
    CloudSettings, Gaussian3d, PlanarGaussian3dHandle, PlanarSync, RadixSortDepthBits,
    render::extract_gaussians,
    sort::{
        GpuSortedEntry, SortedEntriesHandle,
        radix::{RadixSortBuffers, RadixSortPipeline},
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

#[derive(Component)]
pub struct Snapshot;
#[derive(Component)]
pub(super) struct GpuSnapshot;
#[derive(Resource, Clone, Default)]
pub struct ReadySnapshots(pub Arc<Mutex<HashSet<Entity>>>);

#[derive(Resource, Default)]
struct PublishedGpu(HashMap<Entity, AssetId<bevy_gaussian_splatting::PlanarGaussian3d>>);

/// Install the resources and systems owned by this module.
pub fn install(app: &mut App) {
    #[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
    let ready = ReadySnapshots::default();
    #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
    let ready = ReadySnapshots(
        app.world()
            .resource::<bevy_gaussian_splatting::render::webgl::ReadyClouds>()
            .0
            .clone(),
    );
    let transfers = GpuTransfers::default();
    app.insert_resource(ready.clone())
        .insert_resource(transfers.clone())
        .init_resource::<SogRendererBackend>()
        .init_resource::<GpuBudget>()
        .init_resource::<GpuResident>()
        .init_resource::<super::packed::PackedResident>()
        .init_resource::<super::super::runtime::SogRuntimeSettings>()
        .init_resource::<super::super::runtime::SogSelectionStatistics>()
        .add_systems(PostUpdate, (super::gpu::cleanup, super::packed::cleanup));
    #[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app
            .insert_resource(ready)
            .insert_resource(transfers)
            .init_resource::<PublishedGpu>()
            .add_systems(
                ExtractSchedule,
                publish_gpu_assets.before(extract_gaussians::<Gaussian3d>),
            )
            .add_systems(
                ExtractSchedule,
                report_ready_snapshots.after(publish_gpu_assets),
            );
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Bevy injects each independent render-system parameter."
)]
fn publish_gpu_assets(
    transfers: Res<GpuTransfers>,
    queue: Res<RenderQueue>,
    mut published: ResMut<PublishedGpu>,
    mut clouds: ResMut<RenderAssets<<Gaussian3d as PlanarSync>::GpuPlanarType>>,
    mut sorted: ResMut<RenderAssets<GpuSortedEntry>>,
    mut scratch: ResMut<RadixSortBuffers<Gaussian3d>>,
    mut pipeline: ResMut<RadixSortPipeline<Gaussian3d>>,
    cache: Res<PipelineCache>,
) {
    let mut transfers = transfers.0.lock().unwrap();
    for (cloud, sort) in transfers.retire.drain(..) {
        clouds.remove(cloud);
        sorted.remove(sort);
        scratch.release_persistent(cloud);
        published.0.retain(|_, id| *id != cloud);
    }
    for publication in std::mem::take(&mut transfers.publish) {
        // Extraction is synchronized with completion of the previous render CPU
        // schedule. Its old-slot draws have therefore reached this same queue.
        // Submit replacement copies now, never from the concurrently running main
        // update. GPU queue ordering is sufficient; no synchronous GPU wait.
        queue.submit([publication.selection_copies.into_inner().finish()]);
        transfers.submitted_copy_bytes += publication.copy_bytes;
        // The previous entity using this inactive slot was already despawned.
        // A fresh render entity obtains fresh planar and sort bind groups.
        published
            .0
            .insert(publication.entity, publication.cloud_handle.id());
        clouds.insert(publication.cloud_handle.id(), publication.cloud);
        sorted.insert(publication.sort_handle.id(), publication.sorted);
        pipeline.queue_variant(&cache, RadixSortDepthBits::default(), false);
    }
    for publication in std::mem::take(&mut transfers.compact_sorted) {
        scratch.retain_persistent(publication.cloud, publication.capacity);
        sorted.insert(publication.sorted.id(), publication.entries);
    }
}

#[allow(clippy::type_complexity)]
#[allow(
    clippy::too_many_arguments,
    reason = "Bevy injects each independent render-system parameter."
)]
fn report_ready_snapshots(
    ready: Res<ReadySnapshots>,
    mut published: ResMut<PublishedGpu>,
    clouds: Res<RenderAssets<<Gaussian3d as PlanarSync>::GpuPlanarType>>,
    sorted: Res<RenderAssets<GpuSortedEntry>>,
    scratch: Res<RadixSortBuffers<Gaussian3d>>,
    mut pipeline: ResMut<RadixSortPipeline<Gaussian3d>>,
    cache: Res<PipelineCache>,
    compact_clouds: Query<&bevy_gaussian_splatting::render::compact::CompactCloud>,
    snapshots: Extract<
        Query<
            (
                Entity,
                &PlanarGaussian3dHandle,
                &SortedEntriesHandle,
                Has<GpuSnapshot>,
                &CloudSettings,
                Option<&bevy::render::sync_world::RenderEntity>,
                Option<&bevy_gaussian_splatting::render::compact::CompactCloud>,
            ),
            With<Snapshot>,
        >,
    >,
) {
    let mut ready = ready.0.lock().unwrap();
    ready.clear();
    published
        .0
        .retain(|entity, _| snapshots.get(*entity).is_ok());
    for (entity, cloud_handle, entries, gpu, settings, render_entity, compact) in &snapshots {
        if let Some(compact) = compact {
            let render = render_entity.and_then(|entity| compact_clouds.get(entity.id()).ok());
            // Extraction may deliberately choose vertex projection after an
            // experimental pipeline failure. Acknowledgement must follow that
            // actual render path, not strand a ready fallback on requested mode.
            let projected = render.map_or_else(
                || {
                    compact.has_projection_cache()
                        && bevy_gaussian_splatting::render::projected::supports(settings)
                },
                |render| render.has_projection_cache(),
            );
            if settings.stochastic() {
                if render.is_some_and(|render| render.is_prepared())
                    && sorted
                        .get(&entries.0)
                        .is_some_and(|s| s.count == compact.len())
                {
                    ready.insert(entity);
                }
                continue;
            }
            pipeline.queue_projection_variant(
                &cache,
                settings.radix_sort_depth_bits,
                settings.visibility_compaction,
                true,
                projected,
            );
            let prepared = render.is_some_and(|render| render.is_prepared());
            if prepared
                && sorted
                    .get(&entries.0)
                    .is_some_and(|s| s.count == compact.len())
                && scratch
                    .asset_map
                    .get(&cloud_handle.0.id())
                    .is_some_and(|s| s.entry_buffer_b.size() >= compact.len() as u64 * 8)
                && pipeline
                    .variant_for_projection(settings.radix_sort_depth_bits, true, projected)
                    .is_some_and(|v| v.is_loaded(&cache, settings.visibility_compaction))
            {
                ready.insert(entity);
            }
            continue;
        }
        // Hidden pending clouds are omitted by upstream extract_gaussians, so
        // its normal render-world queue cannot request new variants/stages yet.
        // Queue from the unfiltered main-world snapshot query before checking
        // asset readiness; activation must never be required to start compiling.
        if !settings.stochastic() {
            pipeline.queue_variant(
                &cache,
                settings.radix_sort_depth_bits,
                settings.visibility_compaction,
            );
        }
        let Some(cloud) = clouds.get(&cloud_handle.0) else {
            continue;
        };
        let Some(sorted) = sorted.get(&entries.0) else {
            continue;
        };
        let gpu_resources_ready = published.0.get(&entity) == Some(&cloud_handle.0.id())
            && sorted.count == cloud.count
            && scratch
                .asset_map
                .get(&cloud_handle.0.id())
                .is_some_and(|scratch| scratch.entry_buffer_b.size() >= cloud.count as u64 * 8);
        if !snapshot_can_activate(gpu, gpu_resources_ready, settings, |bits, compact| {
            pipeline
                .variant(bits)
                .is_some_and(|variant| variant.is_loaded(&cache, compact))
        }) {
            continue;
        }
        ready.insert(entity);
    }
}

/// Core readiness decision; GPU publication checks remain specific to GPU slots.
fn snapshot_can_activate(
    gpu: bool,
    gpu_resources_ready: bool,
    settings: &CloudSettings,
    pipeline_ready: impl FnOnce(RadixSortDepthBits, bool) -> bool,
) -> bool {
    (!gpu || gpu_resources_ready)
        && (settings.stochastic()
            || pipeline_ready(
                settings.radix_sort_depth_bits,
                settings.visibility_compaction,
            ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_compaction_waits_for_requested_stages_before_acknowledgment() {
        let compact = CloudSettings {
            visibility_compaction: true,
            radix_sort_depth_bits: RadixSortDepthBits::Bits16,
            ..default()
        };
        // Missing, queued/compiling, or failed requested stages all remain unusable.
        for stage_usable in [None, Some(false)] {
            let mut checked = false;
            assert!(!snapshot_can_activate(
                false,
                false,
                &compact,
                |bits, flag| {
                    checked = true;
                    assert_eq!(bits, RadixSortDepthBits::Bits16);
                    assert!(flag);
                    stage_usable == Some(true)
                }
            ));
            assert!(
                checked,
                "snapshot backend must inspect its requested stages"
            );
        }
        assert!(snapshot_can_activate(
            false,
            false,
            &compact,
            |bits, flag| { bits == RadixSortDepthBits::Bits16 && flag }
        ));
        let baseline = CloudSettings {
            visibility_compaction: false,
            ..compact
        };
        assert!(snapshot_can_activate(
            false,
            false,
            &baseline,
            |_, compact| !compact
        ));
    }

    #[test]
    fn gpu_publication_checks_remain_required_alongside_requested_pipeline_readiness() {
        let compact = CloudSettings {
            visibility_compaction: true,
            ..default()
        };
        assert!(!snapshot_can_activate(true, false, &compact, |_, _| true));
        assert!(!snapshot_can_activate(true, true, &compact, |_, _| false));
        assert!(snapshot_can_activate(
            true,
            true,
            &compact,
            |_, compaction| compaction
        ));
    }
}
