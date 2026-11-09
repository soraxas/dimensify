//! Optional bounded acknowledgements of actual point-cloud draw encoding.
use bevy::{
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        Render, RenderApp, RenderSystems,
    },
};
use std::sync::{Arc, Mutex};

#[derive(Component, Clone, Copy, Debug, ExtractComponent)]
pub struct PointCloudDiagnosticSample(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointCloudViewStatus {
    pub view: Entity,
    pub sample_id: u64,
    pub drawn_chunks: usize,
    pub drawn_points: usize,
    pub overflow: bool,
}

struct TrackedView {
    snapshot: PointCloudViewStatus,
    chunks: Vec<Entity>,
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct PointCloudRenderStatus(Arc<Mutex<Vec<TrackedView>>>);

impl PointCloudRenderStatus {
    pub fn snapshots(&self) -> Vec<PointCloudViewStatus> {
        match self.0.lock() {
            Ok(views) => views.iter().map(|v| v.snapshot).collect(),
            Err(_) => Vec::new(), // fail closed after a poisoned render bridge
        }
    }

    pub(crate) fn begin_frame(
        &self,
        samples: impl IntoIterator<Item = (Entity, PointCloudDiagnosticSample)>,
    ) {
        let Ok(mut views) = self.0.lock() else {
            return;
        };
        views.clear();
        views.extend(
            samples
                .into_iter()
                .take(16)
                .map(|(view, sample)| TrackedView {
                    snapshot: PointCloudViewStatus {
                        view,
                        sample_id: sample.0,
                        drawn_chunks: 0,
                        drawn_points: 0,
                        overflow: false,
                    },
                    chunks: Vec::new(),
                }),
        );
    }

    pub(crate) fn record_draw(&self, view: Entity, sample: u64, chunk: Entity, count: usize) {
        let Ok(mut views) = self.0.lock() else {
            return;
        };
        let Some(tracked) = views
            .iter_mut()
            .find(|v| v.snapshot.view == view && v.snapshot.sample_id == sample)
        else {
            return;
        };
        if tracked.chunks.contains(&chunk) {
            return;
        }
        if tracked.chunks.len() >= 4096 {
            tracked.snapshot.overflow = true;
            return;
        }
        let Some(total) = tracked.snapshot.drawn_points.checked_add(count) else {
            tracked.snapshot.overflow = true;
            return;
        };
        tracked.chunks.push(chunk);
        tracked.snapshot.drawn_chunks += 1;
        tracked.snapshot.drawn_points = total;
    }
}

pub(crate) struct PointCloudRenderStatusPlugin;
impl Plugin for PointCloudRenderStatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointCloudRenderStatus>().add_plugins((
            ExtractResourcePlugin::<PointCloudRenderStatus>::default(),
            ExtractComponentPlugin::<PointCloudDiagnosticSample>::default(),
        ));
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(Render, reset_status.in_set(RenderSystems::PrepareViews));
        }
    }
}

fn reset_status(
    status: Res<PointCloudRenderStatus>,
    views: Query<(Entity, &PointCloudDiagnosticSample)>,
) {
    status.begin_frame(views.iter().map(|(view, sample)| (view, *sample)));
}

#[cfg(test)]
mod tests {
    use super::{PointCloudDiagnosticSample, PointCloudRenderStatus};
    use bevy::prelude::*;

    #[test]
    fn only_matching_successful_draws_count_and_reset_each_frame() {
        let mut world = World::new();
        let view = world.spawn_empty().id();
        let chunk = world.spawn_empty().id();
        let status = PointCloudRenderStatus::default();
        status.begin_frame([(view, PointCloudDiagnosticSample(7))]);
        status.record_draw(view, 6, chunk, 100);
        assert_eq!(status.snapshots()[0].drawn_points, 0);
        status.record_draw(view, 7, chunk, 100);
        status.record_draw(view, 7, chunk, 100);
        assert_eq!(status.snapshots()[0].drawn_points, 100);
        assert_eq!(status.snapshots()[0].drawn_chunks, 1);
        status.begin_frame([(view, PointCloudDiagnosticSample(8))]);
        assert_eq!(status.snapshots()[0].drawn_points, 0);
        assert_eq!(status.snapshots()[0].sample_id, 8);
    }
}
