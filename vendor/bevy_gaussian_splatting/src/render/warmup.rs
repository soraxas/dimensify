//! Compile a cloud's pipelines before its data arrives.
//!
//! Bevy compiles pipelines synchronously on macOS, so without warm-up the first
//! frame that draws a new cloud waits on every variant it needs. Attach
//! [`CloudPipelineWarmup`] to a never-drawn entity that carries the
//! [`CloudSettings`] a cloud will have, for example a hidden child of a
//! streamed scene root that the application stamps exactly like its clouds.
//! Each frame the same keys a real cloud would request are specialized through
//! [`super::cloud_pipeline_key`], so the first real draw finds them compiled.
use bevy::{
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_resource::{PipelineCache, SpecializedRenderPipelines},
        view::ExtractedView,
    },
};

use super::{
    CloudPipeline, cloud_pipeline_key, compact, compositing::CompositingViews, depth_pass,
    depth_pass_key, transparency_sibling,
};
use crate::{CloudSettings, Gaussian3d, GaussianCamera, gaussian::settings::GaussianMode};

/// Warm the pipelines a 3D cloud with this entity's [`CloudSettings`] will use.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CloudPipelineWarmup {
    /// The cloud will use compact (packed streaming) work.
    pub compact: bool,
    /// The compact cloud will use the projection cache.
    pub projected: bool,
}

/// Extracts warm-up templates and specializes their 3D pipelines each frame.
pub struct WarmupPlugin;

impl Plugin for WarmupPlugin {
    fn build(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<WarmupTemplates>()
            .add_systems(ExtractSchedule, extract_warmups)
            .add_systems(Render, warm_cloud_pipelines.in_set(RenderSystems::Queue));
    }
}

#[derive(Resource, Default)]
struct WarmupTemplates(Vec<(CloudSettings, CloudPipelineWarmup)>);

fn extract_warmups(
    mut templates: ResMut<WarmupTemplates>,
    warmups: Extract<Query<(&CloudSettings, &CloudPipelineWarmup)>>,
) {
    templates.0.clear();
    templates.0.extend(
        warmups
            .iter()
            .map(|(settings, warmup)| (settings.clone(), *warmup)),
    );
}

#[allow(clippy::too_many_arguments)]
fn warm_cloud_pipelines(
    templates: Res<WarmupTemplates>,
    custom: Res<CloudPipeline<Gaussian3d>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<CloudPipeline<Gaussian3d>>>,
    cache: Res<PipelineCache>,
    compositing: Option<Res<CompositingViews>>,
    depth_request: Option<Res<depth_pass::GaussianDepthPass>>,
    #[cfg(all(feature = "sort_radix", not(feature = "buffer_texture")))] mut radix: Option<
        ResMut<crate::sort::radix::RadixSortPipeline<Gaussian3d>>,
    >,
    views: Query<(Entity, &ExtractedView, Option<&Msaa>), With<GaussianCamera>>,
) {
    for (settings, warmup) in &templates.0 {
        if settings.gaussian_mode != GaussianMode::Gaussian3d {
            continue;
        }
        let compact = warmup.compact && compact::supports(settings);
        let projected = compact && warmup.projected && super::projected::supports(settings);
        for (view_entity, view, msaa) in &views {
            // What `queue_gaussians` specializes for a drawn cloud.
            let output_srgb = compositing
                .as_ref()
                .is_some_and(|views| views.ready(view_entity));
            let key = cloud_pipeline_key(settings, compact, projected, view, msaa, output_srgb);
            pipelines.specialize(&cache, &custom, key);
            if settings.transparency_warmup && settings.stochastic_supported() {
                pipelines.specialize(&cache, &custom, transparency_sibling(key));
            }
            if let Some(request) = depth_request.as_deref()
                && request.enabled
                && !key.stochastic
            {
                pipelines.specialize(&cache, &custom, depth_pass_key(key, request));
            }
            // What `prepare_compact_pipelines` specializes before handoff.
            if compact {
                let linear = cloud_pipeline_key(settings, true, projected, view, msaa, false);
                pipelines.specialize(&cache, &custom, linear);
            }
        }
        #[cfg(all(feature = "sort_radix", not(feature = "buffer_texture")))]
        if let Some(radix) = radix.as_mut()
            && !settings.stochastic()
        {
            radix.queue_projection_variant(
                &cache,
                settings.radix_sort_depth_bits,
                settings.visibility_compaction,
                compact,
                projected,
            );
        }
    }
}
