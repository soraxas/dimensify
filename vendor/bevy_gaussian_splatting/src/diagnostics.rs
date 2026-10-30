//! Optional, bounded render acknowledgements for Gaussian benchmark cameras.
//!
//! This is renderer-owned so a standalone harness can observe actual encoding
//! without introducing any dependency from the renderer back to the harness.

use std::sync::{Arc, Mutex};

use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        view::RenderVisibleEntities,
    },
};

use crate::gaussian::cloud::CloudVisibilityClass;

// An explicit cap prevents a pathological camera/cloud scene from growing the
// shared snapshot without limit. Overflow is never reported as ready.
const MAX_TRACKED_VIEWS: usize = 16;
const MAX_TRACKED_CLOUDS_PER_VIEW: usize = 4096;

pub use crate::timestamps::{GaussianTimestampInterval, radix_timestamp_indices};

/// Optional per-view compute-pass descriptors supplied by a diagnostic consumer.
/// Nothing is installed by the product renderer unless a consumer opts in.
#[derive(Resource, Default)]
pub struct GaussianTimestampWrites(
    pub std::collections::HashMap<Entity, GaussianTimestampInterval>,
);

/// Identity of the current benchmark sample, attached to a Gaussian camera.
#[derive(Component, Clone, Copy, Debug, ExtractComponent)]
pub struct GaussianDiagnosticSample(pub u64);

/// Per-view acknowledgement of commands encoded for one sample in one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GaussianViewStatus {
    pub view: Entity,
    pub sample_id: u64,
    pub cloud_count: usize,
    pub sort_encoded: usize,
    pub draw_encoded: usize,
    /// Actual draws using the compute-projected compact layout, after fallback.
    pub projected_draw_encoded: usize,
    /// True only if every visible cloud was both sorted and drawn this frame.
    pub ready: bool,
    /// Every visible cloud was sorted and drawn using compute-projected records.
    pub projected_ready: bool,
}

#[derive(Clone, Copy)]
struct CloudAcknowledgement {
    entity: Entity,
    sort_encoded: bool,
    draw_encoded: bool,
    projected_draw_encoded: bool,
}

struct TrackedView {
    view: Entity,
    sample_id: u64,
    cloud_count: usize,
    clouds: Vec<CloudAcknowledgement>,
}

impl TrackedView {
    fn snapshot(&self) -> GaussianViewStatus {
        let sort_encoded = self
            .clouds
            .iter()
            .filter(|cloud| cloud.sort_encoded)
            .count();
        let draw_encoded = self
            .clouds
            .iter()
            .filter(|cloud| cloud.draw_encoded)
            .count();
        let projected_draw_encoded = self
            .clouds
            .iter()
            .filter(|cloud| cloud.draw_encoded && cloud.projected_draw_encoded)
            .count();
        let ready = self.cloud_count > 0
            && self.cloud_count == self.clouds.len()
            && self
                .clouds
                .iter()
                .all(|cloud| cloud.sort_encoded && cloud.draw_encoded);
        GaussianViewStatus {
            view: self.view,
            sample_id: self.sample_id,
            cloud_count: self.cloud_count,
            sort_encoded,
            draw_encoded,
            projected_draw_encoded,
            ready,
            projected_ready: ready && projected_draw_encoded == self.cloud_count,
        }
    }
}

/// Shared, bounded snapshot read in the main world and written in the render world.
///
/// `snapshot(view)` returns `None` when the view has no current sample or the
/// fixed view cap is exceeded. A snapshot is replaced at each render frame;
/// callers must match `sample_id` rather than carrying a prior ready flag.
#[derive(Resource, Clone, Default, ExtractResource)]
pub struct GaussianRenderStatus(Arc<Mutex<Vec<TrackedView>>>);

impl GaussianRenderStatus {
    /// Read the latest status for one render-view entity.
    pub fn snapshot(&self, view: Entity) -> Option<GaussianViewStatus> {
        let views = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        views
            .iter()
            .find(|tracked| tracked.view == view)
            .map(TrackedView::snapshot)
    }

    /// Read all current sampled views (at most 16), including render entity IDs.
    pub fn snapshots(&self) -> Vec<GaussianViewStatus> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(TrackedView::snapshot)
            .collect()
    }

    pub(crate) fn begin_frame(
        &self,
        sampled_views: impl IntoIterator<Item = (Entity, GaussianDiagnosticSample, Vec<Entity>)>,
    ) {
        let mut views = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        views.clear();
        for (view, sample, expected_clouds) in sampled_views.into_iter().take(MAX_TRACKED_VIEWS) {
            let cloud_count = expected_clouds.len();
            let clouds = expected_clouds
                .into_iter()
                .take(MAX_TRACKED_CLOUDS_PER_VIEW)
                .map(|entity| CloudAcknowledgement {
                    entity,
                    sort_encoded: false,
                    draw_encoded: false,
                    projected_draw_encoded: false,
                })
                .collect();
            views.push(TrackedView {
                view,
                sample_id: sample.0,
                cloud_count,
                clouds,
            });
        }
    }

    pub(crate) fn record_sort(&self, view: Entity, sample_id: u64, cloud: Entity) {
        self.acknowledge(view, sample_id, cloud, |ack| ack.sort_encoded = true);
    }

    pub(crate) fn record_draw(&self, view: Entity, sample_id: u64, cloud: Entity, projected: bool) {
        self.acknowledge(view, sample_id, cloud, |ack| {
            ack.draw_encoded = true;
            ack.projected_draw_encoded = projected;
        });
    }

    fn acknowledge(
        &self,
        view: Entity,
        sample_id: u64,
        cloud: Entity,
        update: impl FnOnce(&mut CloudAcknowledgement),
    ) {
        let mut views = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ack) = views
            .iter_mut()
            .find(|tracked| tracked.view == view && tracked.sample_id == sample_id)
            .and_then(|tracked| tracked.clouds.iter_mut().find(|ack| ack.entity == cloud))
        {
            update(ack);
        }
    }
}

pub(crate) struct GaussianRenderDiagnosticsPlugin;

impl Plugin for GaussianRenderDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GaussianRenderStatus>().add_plugins((
            ExtractResourcePlugin::<GaussianRenderStatus>::default(),
            ExtractComponentPlugin::<GaussianDiagnosticSample>::default(),
        ));
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                reset_render_status.in_set(RenderSystems::PrepareViews),
            );
        }
    }
}

fn reset_render_status(
    status: Res<GaussianRenderStatus>,
    views: Query<(Entity, &GaussianDiagnosticSample, &RenderVisibleEntities)>,
) {
    status.begin_frame(views.iter().map(|(view, sample, visible)| {
        let clouds = visible
            .get::<CloudVisibilityClass>()
            .map(|class| {
                class
                    .entities_cpu_culling
                    .iter()
                    .map(|(render_entity, _)| *render_entity)
                    .collect()
            })
            .unwrap_or_default();
        (view, *sample, clouds)
    }));
}

#[cfg(test)]
mod tests {
    use super::{GaussianDiagnosticSample, GaussianRenderStatus};
    use bevy::prelude::*;

    #[test]
    fn timestamp_indices_enclose_reset_through_final_digit_only() {
        assert_eq!(
            super::radix_timestamp_indices(4, 5, None, 4),
            (Some(4), None)
        );
        assert_eq!(
            super::radix_timestamp_indices(4, 5, Some(0), 4),
            (None, None)
        );
        assert_eq!(
            super::radix_timestamp_indices(4, 5, Some(2), 4),
            (None, None)
        );
        assert_eq!(
            super::radix_timestamp_indices(4, 5, Some(3), 4),
            (None, Some(5))
        );
        assert_eq!(
            super::radix_timestamp_indices(4, 5, Some(0), 1),
            (None, Some(5))
        );
    }

    #[test]
    fn different_clouds_cannot_combine_into_ready() {
        let status = GaussianRenderStatus::default();
        let mut world = World::new();
        let view = world.spawn_empty().id();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        status.begin_frame([(view, GaussianDiagnosticSample(7), vec![first, second])]);

        status.record_sort(view, 7, first);
        status.record_draw(view, 7, second, false);
        let snapshot = status.snapshot(view).expect("view should be tracked");
        assert_eq!(snapshot.cloud_count, 2);
        assert_eq!(snapshot.sort_encoded, 1);
        assert_eq!(snapshot.draw_encoded, 1);
        assert!(!snapshot.ready);

        status.record_draw(view, 7, first, false);
        status.record_sort(view, 7, second);
        let snapshot = status.snapshot(view).expect("view should be tracked");
        assert_eq!(snapshot.sort_encoded, 2);
        assert_eq!(snapshot.draw_encoded, 2);
        assert!(snapshot.ready);
    }

    #[test]
    fn vertex_fallback_draw_does_not_acknowledge_projected_ready() {
        let status = GaussianRenderStatus::default();
        let mut world = World::new();
        let view = world.spawn_empty().id();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        status.begin_frame([(view, GaussianDiagnosticSample(21), vec![first, second])]);
        for cloud in [first, second] {
            status.record_sort(view, 21, cloud);
        }
        status.record_draw(view, 21, first, true);
        status.record_draw(view, 21, second, false);
        let fallback = status.snapshot(view).expect("tracked view");
        assert!(fallback.ready);
        assert_eq!(fallback.projected_draw_encoded, 1);
        assert!(
            !fallback.projected_ready,
            "ordinary fallback must not claim projected draws"
        );

        status.begin_frame([(view, GaussianDiagnosticSample(22), vec![first, second])]);
        let reset = status.snapshot(view).expect("tracked view");
        assert_eq!(reset.projected_draw_encoded, 0);
        assert!(!reset.projected_ready);
        for cloud in [first, second] {
            status.record_sort(view, 22, cloud);
            status.record_draw(view, 22, cloud, true);
        }
        let projected = status.snapshot(view).expect("tracked view");
        assert!(projected.ready && projected.projected_ready);
        assert_eq!(projected.projected_draw_encoded, projected.cloud_count);
    }

    #[test]
    fn missing_radix_pipeline_never_readies_after_draw() {
        let status = GaussianRenderStatus::default();
        let mut world = World::new();
        let view = world.spawn_empty().id();
        let cloud = world.spawn_empty().id();
        status.begin_frame([(view, GaussianDiagnosticSample(11), vec![cloud])]);
        status.record_draw(view, 11, cloud, false);
        let snapshot = status.snapshot(view).expect("view should be tracked");
        assert_eq!(snapshot.sort_encoded, 0);
        assert_eq!(snapshot.draw_encoded, 1);
        assert!(!snapshot.ready);
    }

    #[test]
    fn missing_draw_pipeline_never_readies_after_sort() {
        let status = GaussianRenderStatus::default();
        let mut world = World::new();
        let view = world.spawn_empty().id();
        let cloud = world.spawn_empty().id();
        status.begin_frame([(view, GaussianDiagnosticSample(12), vec![cloud])]);
        status.record_sort(view, 12, cloud);
        let snapshot = status.snapshot(view).expect("view should be tracked");
        assert_eq!(snapshot.sort_encoded, 1);
        assert_eq!(snapshot.draw_encoded, 0);
        assert!(!snapshot.ready);
    }

    #[test]
    fn acknowledgements_are_idempotent_and_reset_each_frame() {
        let status = GaussianRenderStatus::default();
        let mut world = World::new();
        let view = world.spawn_empty().id();
        let other_view = world.spawn_empty().id();
        let cloud = world.spawn_empty().id();
        let other_cloud = world.spawn_empty().id();
        status.begin_frame([
            (view, GaussianDiagnosticSample(15), vec![cloud]),
            (other_view, GaussianDiagnosticSample(15), vec![other_cloud]),
        ]);
        status.record_sort(view, 15, cloud);
        status.record_sort(view, 15, cloud);
        status.record_draw(view, 14, cloud, false);
        status.record_draw(view, 15, other_cloud, false);
        status.record_draw(other_view, 15, cloud, false);
        let snapshot = status.snapshot(view).expect("view should be tracked");
        assert_eq!(snapshot.sort_encoded, 1);
        assert_eq!(snapshot.draw_encoded, 0);
        assert!(!snapshot.ready);

        status.record_draw(view, 15, cloud, false);
        assert!(status.snapshot(view).expect("view should be tracked").ready);
        status.begin_frame([(view, GaussianDiagnosticSample(15), vec![cloud])]);
        let snapshot = status.snapshot(view).expect("view should be tracked");
        assert_eq!(snapshot.sort_encoded, 0);
        assert_eq!(snapshot.draw_encoded, 0);
        assert!(!snapshot.ready);
        assert!(status.snapshot(other_view).is_none());
        assert_eq!(status.snapshots(), vec![snapshot]);
    }

    #[test]
    fn snapshot_caps_views_and_clouds_without_false_ready() {
        let status = GaussianRenderStatus::default();
        let mut world = World::new();
        let views: Vec<_> = (0..17).map(|_| world.spawn_empty().id()).collect();
        let clouds: Vec<_> = (0..4097).map(|_| world.spawn_empty().id()).collect();
        status.begin_frame(
            views
                .iter()
                .copied()
                .map(|view| (view, GaussianDiagnosticSample(20), clouds.clone())),
        );

        for cloud in &clouds {
            status.record_sort(views[0], 20, *cloud);
            status.record_draw(views[0], 20, *cloud, false);
        }
        let snapshot = status.snapshot(views[0]).expect("view should be tracked");
        assert_eq!(snapshot.cloud_count, 4097);
        assert_eq!(snapshot.sort_encoded, 4096);
        assert_eq!(snapshot.draw_encoded, 4096);
        assert!(!snapshot.ready);
        assert_eq!(status.snapshots().len(), 16);
        assert!(status.snapshot(views[16]).is_none());
    }
}
