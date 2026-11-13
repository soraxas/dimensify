//! On-demand, depth-only splat pass for screen-space picking.
//!
//! Sorted splats blend without writing depth, so a depth-buffer pick finds
//! nothing. For views listed in [`GaussianDepthPass`], the queued splats are
//! drawn a second time after the main pass with colour writes disabled, writing
//! depth only where a splat's alpha reaches [`GaussianDepthPass::alpha`]. The pass runs
//! only on frames that request it; ordinary frames draw nothing extra. Stochastic
//! clouds already write depth and are skipped.
use bevy::{
    core_pipeline::{
        core_3d::Transparent3d,
        schedule::{Core3d, Core3dSystems},
    },
    platform::collections::HashMap,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        render_phase::SortedRenderPhase,
        render_resource::{RenderPassDescriptor, StoreOp},
        renderer::{RenderContext, ViewQuery},
        sync_world::MainEntity,
        view::{ViewDepthTexture, ViewTarget},
    },
};

/// Default [`GaussianDepthPass::alpha`].
pub const DEPTH_PASS_ALPHA: f32 = 0.25;

/// Render-world request: draw splat depth for these main-world views this frame.
#[derive(Resource, Clone, Debug)]
pub struct GaussianDepthPass {
    /// Keep depth-pass pipelines specialized so the first request does not wait
    /// on shader compilation. Nothing is drawn unless `views` names the view.
    pub enabled: bool,
    pub views: Vec<MainEntity>,
    /// Minimum single-splat alpha that writes pick depth (see `DEPTH_PASS` in
    /// `gaussian.wgsl`), clamped to `0..=1`. Lower values catch faint surfaces
    /// but also floaters. The threshold is a pipeline constant, so each new
    /// value compiles a pipeline variant; picks miss until it is ready.
    pub alpha: f32,
}

impl Default for GaussianDepthPass {
    fn default() -> Self {
        Self {
            enabled: false,
            views: Vec::new(),
            alpha: DEPTH_PASS_ALPHA,
        }
    }
}

impl GaussianDepthPass {
    /// Shader constant for [`Self::alpha`]; non-finite values use the default.
    pub(super) fn alpha_bits(&self) -> u32 {
        let alpha = if self.alpha.is_finite() {
            self.alpha.clamp(0.0, 1.0)
        } else {
            DEPTH_PASS_ALPHA
        };
        alpha.to_bits()
    }

    pub(super) fn draws(&self, view: MainEntity) -> bool {
        self.enabled && self.views.contains(&view)
    }
}

/// Depth-only items queued per render view for the current frame.
#[derive(Resource, Default)]
pub(super) struct GaussianDepthPhases(pub HashMap<Entity, SortedRenderPhase<Transparent3d>>);

/// Draws requested splat depth after the main pass. Order depth consumers after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GaussianDepthPassSystems;

pub(super) fn install(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .init_resource::<GaussianDepthPass>()
        .init_resource::<GaussianDepthPhases>()
        .configure_sets(
            Core3d,
            GaussianDepthPassSystems
                .after(Core3dSystems::MainPass)
                .before(Core3dSystems::EarlyPostProcess),
        )
        .add_systems(Core3d, draw_depth_pass.in_set(GaussianDepthPassSystems))
        .add_systems(Render, clear_depth_phases.in_set(RenderSystems::Queue));
}

/// Clear last frame's depth items once, before every format's queue system adds
/// its own; a clear inside each `queue_gaussians::<R>` would drop the others'.
pub(super) fn clear_depth_phases(mut phases: ResMut<GaussianDepthPhases>) {
    phases.0.clear();
}

fn draw_depth_pass(
    world: &World,
    view: ViewQuery<(&ExtractedCamera, &ViewTarget, &ViewDepthTexture)>,
    phases: Res<GaussianDepthPhases>,
    mut ctx: RenderContext,
) {
    let entity = view.entity();
    let Some(phase) = phases
        .0
        .get(&entity)
        .filter(|phase| !phase.items.is_empty())
    else {
        return;
    };
    let (camera, target, depth) = view.into_inner();
    // The pipeline masks colour writes; the target is bound only so the
    // attachment layout matches the main splat pipeline.
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("gaussian_depth_pass"),
        color_attachments: &[Some(target.get_color_attachment())],
        depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    if let Some(viewport) = camera.viewport.as_ref() {
        pass.set_camera_viewport(viewport);
    }
    if let Err(error) = phase.render(&mut pass, world, entity) {
        error!("Gaussian depth pass failed: {error:?}");
    }
}
