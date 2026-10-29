//! Timestamp existing passes only when their complete draw range has one tagged
//! Gaussian identity. Never attribute unrelated transparent meshes to a scene.
use super::{GaussianGpuTimings, GaussianTimingSource, GaussianTimingView, PassScope, Recorder};
use crate::{render::CloudUniform, timestamps::GaussianTimestampInterval};
use bevy::{
    camera::MainPassResolutionOverride,
    core_pipeline::{core_3d::Transparent3d, oit::OrderIndependentTransparencySettings},
    prelude::*,
    render::{
        camera::ExtractedCamera,
        render_phase::SortedRenderPhase,
        render_resource::{RenderPassDescriptor, StoreOp},
        renderer::RenderContext,
        view::{ViewDepthTexture, ViewTarget},
    },
};
use std::ops::Range;

pub(crate) fn interval(
    world: &World,
    view: Entity,
    phase: &SortedRenderPhase<Transparent3d>,
    range: Range<usize>,
    scope: PassScope,
) -> Option<GaussianTimestampInterval> {
    world.get::<GaussianTimingView>(view)?;
    let bridge = world.get_resource::<GaussianGpuTimings>()?;
    let recorder = world.get_resource::<Recorder>()?;
    let mut attribution = Attribution::default();
    for i in range {
        let item = &phase.items[i];
        if item.batch_range.is_empty() {
            continue;
        }
        // Current Gaussian draws have one phase item each. A future cross-entity
        // batching policy must supply its own exact source attribution.
        if item.batch_range.len() != 1 {
            return None;
        }
        let next = *world.get::<GaussianTimingSource>(item.entity.0)?;
        let uniform = world.get::<CloudUniform>(item.entity.0)?;
        attribution.include(
            next,
            uniform.count as usize,
            uniform.visibility_compaction != 0,
        )?;
    }
    recorder.record_pass(
        bridge,
        scope,
        view,
        attribution.source?,
        attribution.count,
        attribution.compact?,
    )
}

#[derive(Default)]
struct Attribution {
    source: Option<GaussianTimingSource>,
    count: usize,
    compact: Option<bool>,
}

impl Attribution {
    fn include(&mut self, source: GaussianTimingSource, count: usize, compact: bool) -> Option<()> {
        if self
            .source
            .is_some_and(|old| old.scene != source.scene || old.selection != source.selection)
            || self.compact.is_some_and(|old| old != compact)
        {
            return None;
        }
        self.count = self.count.checked_add(count)?;
        self.source = Some(source);
        self.compact = Some(compact);
        Some(())
    }
}

/// Mirrors Bevy's ordinary transparent pass only for a fully attributed range.
/// OIT and resolution-override paths retain Bevy's own implementation and are
/// explicitly outside this collector's supported raster scope.
#[allow(clippy::too_many_arguments)]
pub(crate) fn linear(
    world: &World,
    view: Entity,
    phase: &SortedRenderPhase<Transparent3d>,
    camera: &ExtractedCamera,
    target: &ViewTarget,
    depth: &ViewDepthTexture,
    context: &mut RenderContext,
) -> bool {
    if world
        .get::<OrderIndependentTransparencySettings>(view)
        .is_some()
        || world.get::<MainPassResolutionOverride>(view).is_some()
    {
        return false;
    }
    let Some(timing) = interval(world, view, phase, 0..phase.items.len(), PassScope::Raster) else {
        return false;
    };
    let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("Gaussian timed linear transparency"),
        color_attachments: &[Some(target.get_color_attachment())],
        depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
        timestamp_writes: Some(timing.render_writes()),
        occlusion_query_set: None,
        multiview_mask: None,
    });
    if let Some(viewport) = camera.viewport.as_ref() {
        pass.set_camera_viewport(viewport);
    }
    if let Err(error) = phase.render(&mut pass, world, view) {
        error!("Error rendering timed Gaussian transparency: {error:?}");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_range_attribution_requires_matching_scene_selection_and_policy() {
        let source = GaussianTimingSource {
            scene: 7,
            selection: 11,
        };
        let mut attribution = Attribution::default();
        assert!(
            attribution.source.is_none(),
            "empty ranges have no sample identity"
        );
        assert_eq!(attribution.include(source, 32, true), Some(()));
        assert_eq!(attribution.include(source, 64, true), Some(()));
        assert_eq!(attribution.count, 96);
        assert_eq!(
            attribution.include(GaussianTimingSource { scene: 8, ..source }, 1, true),
            None
        );
        assert_eq!(
            attribution.include(
                GaussianTimingSource {
                    selection: 12,
                    ..source
                },
                1,
                true
            ),
            None
        );
        assert_eq!(attribution.include(source, 1, false), None);
        assert_eq!(attribution.include(source, usize::MAX, true), None);
        assert_eq!(
            attribution.count, 96,
            "rejected ranges cannot change the row count"
        );
    }
}
