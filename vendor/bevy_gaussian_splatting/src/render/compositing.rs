//! Opt-in encoded-colour compositing within the sorted transparent phase.

mod feedback;
mod schedule;
pub use feedback::{CompositingFallback, CompositingFeedback, CompositingStatus};

use bevy::{
    camera::MainPassResolutionOverride,
    core_pipeline::{
        core_3d::{Transparent3d, main_opaque_pass_3d, main_transparent_pass_3d},
        oit::OrderIndependentTransparencySettings,
        schedule::{Core3d, Core3dSystems},
    },
    ecs::schedule::Schedules,
    prelude::*,
    render::{
        Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_phase::ViewSortedRenderPhases,
        render_resource::{
            binding_types::{texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::{ExtractedView, ViewDepthTexture, ViewTarget},
    },
};
use std::collections::{HashMap, HashSet};

/// Requests encoded-space Gaussian blending for this camera. Unsupported
/// target configurations keep ordinary linear blending.
#[derive(Component, Clone, Copy, Default, ExtractComponent)]
#[require(CompositingFeedback)]
pub struct EncodedGaussianCompositing;

pub(crate) struct CompositingPlugin;

impl Plugin for CompositingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<EncodedGaussianCompositing>::default());
        app.add_plugins(ExtractComponentPlugin::<CompositingFeedback>::default());
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<CompositingViews>()
                .add_systems(RenderStartup, initialize)
                .add_systems(
                    Render,
                    prepare
                        .in_set(RenderSystems::Queue)
                        .before(super::queue_gaussians::<crate::Gaussian3d>)
                        .before(super::queue_gaussians::<crate::Gaussian4d>),
                );
        }
    }

    fn finish(&self, app: &mut App) {
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        // Schedule initialization may itself access Schedules. Keep that
        // resource in the world and temporarily take only this schedule.
        let world = render.world_mut();
        let Some(mut core) = world.resource_mut::<Schedules>().remove(Core3d) else {
            warn!("Encoded Gaussian compositing unavailable: Core3d schedule is missing");
            return;
        };
        let result =
            schedule::replace_pass(&mut core, world, main_transparent_pass_3d, render_encoded);
        if result.is_ok() {
            core.configure_sets(
                schedule::CompositingPass
                    .in_set(Core3dSystems::MainPass)
                    .after(main_opaque_pass_3d),
            );
        }
        world.resource_mut::<Schedules>().insert(core);
        match result {
            Ok(()) => {
                render.insert_resource(Installed);
            }
            Err(error) => warn!("Encoded Gaussian compositing unavailable: {error}"),
        }
    }
}

#[derive(Resource)]
struct Installed;

#[derive(Clone, Copy, ShaderType, PartialEq)]
struct TransferRect {
    bounds: UVec4,
}

#[derive(Resource)]
struct TransferPipelines {
    layout: BindGroupLayoutDescriptor,
    encode: CachedRenderPipelineId,
    decode: CachedRenderPipelineId,
}

struct PreparedView {
    rect: TransferRect,
    uniform: UniformBuffer<TransferRect>,
    encode: RenderPipeline,
    decode: RenderPipeline,
    encoded_pipelines: HashSet<CachedRenderPipelineId>,
    linear_fallback_draws: u32,
}

#[derive(Resource, Default)]
pub(super) struct CompositingViews(HashMap<Entity, PreparedView>);

impl CompositingViews {
    pub(super) fn ready(&self, view: Entity) -> bool {
        self.0.contains_key(&view)
    }

    pub(super) fn record(&mut self, view: Entity, pipeline: CachedRenderPipelineId) {
        if let Some(prepared) = self.0.get_mut(&view) {
            prepared.encoded_pipelines.insert(pipeline);
        }
    }

    pub(super) fn record_linear_fallback(&mut self, view: Entity) {
        if let Some(prepared) = self.0.get_mut(&view) {
            prepared.linear_fallback_draws = prepared.linear_fallback_draws.saturating_add(1);
        }
    }
}

fn initialize(mut commands: Commands, cache: Res<PipelineCache>) {
    let layout = BindGroupLayoutDescriptor::new(
        "Gaussian colour transfer",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: false }),
                uniform_buffer::<TransferRect>(false),
            ),
        ),
    );
    let pipeline = |entry: &'static str| {
        cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some(format!("Gaussian colour {entry}").into()),
            layout: vec![layout.clone()],
            vertex: VertexState {
                shader: super::COMPOSITING_TRANSFER_SHADER_HANDLE,
                shader_defs: super::shader_defs(super::CloudPipelineKey::default()),
                entry_point: Some("vertex".into()),
                ..default()
            },
            fragment: Some(FragmentState {
                shader: super::COMPOSITING_TRANSFER_SHADER_HANDLE,
                shader_defs: super::shader_defs(super::CloudPipelineKey::default()),
                entry_point: Some(entry.into()),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            ..default()
        })
    };
    commands.insert_resource(TransferPipelines {
        encode: pipeline("encode"),
        decode: pipeline("decode"),
        layout,
    });
}

fn prepare(
    installed: Option<Res<Installed>>,
    pipelines: Res<TransferPipelines>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut prepared: ResMut<CompositingViews>,
    views: Query<(
        Entity,
        &ViewTarget,
        &ExtractedCamera,
        &Msaa,
        Has<EncodedGaussianCompositing>,
        Has<OrderIndependentTransparencySettings>,
        Has<MainPassResolutionOverride>,
        Option<&CompositingFeedback>,
    )>,
) {
    let ready = installed.is_some().then(|| {
        (
            cache.get_render_pipeline(pipelines.encode),
            cache.get_render_pipeline(pipelines.decode),
        )
    });
    prepared.0.retain(|entity, _| {
        views
            .get(*entity)
            .is_ok_and(|(_, _, _, _, requested, _, _, _)| requested)
    });
    for (entity, target, camera, msaa, requested, oit, override_, feedback) in &views {
        if !requested {
            continue;
        }
        let fallback = if installed.is_none() {
            Some(CompositingFallback::PassUnavailable)
        } else if oit {
            Some(CompositingFallback::Oit)
        } else if override_ {
            Some(CompositingFallback::ResolutionOverride)
        } else if msaa.samples() != 1 {
            Some(CompositingFallback::Msaa)
        } else if target.main_texture_format() != TextureFormat::Rgba16Float {
            Some(CompositingFallback::TargetFormat)
        } else if !matches!(ready, Some((Some(_), Some(_)))) {
            Some(CompositingFallback::TransferPipeline)
        } else {
            None
        };
        if let Some(reason) = fallback {
            prepared.0.remove(&entity);
            if let Some(feedback) = feedback {
                feedback.publish(CompositingStatus::LinearFallback(reason));
            }
            continue;
        }
        let Some((Some(encode), Some(decode))) = ready else {
            continue;
        };
        let size = UVec2::new(
            target.main_texture().width(),
            target.main_texture().height(),
        );
        let (min, max) = camera
            .viewport
            .as_ref()
            .map_or((UVec2::ZERO, size), |viewport| {
                (
                    viewport.physical_position.min(size),
                    viewport
                        .physical_position
                        .saturating_add(viewport.physical_size)
                        .min(size),
                )
            });
        let rect = TransferRect {
            bounds: UVec4::new(min.x, min.y, max.x, max.y),
        };
        let view = prepared.0.entry(entity).or_insert_with(|| {
            let mut uniform = UniformBuffer::from(rect);
            uniform.write_buffer(&device, &queue);
            PreparedView {
                rect,
                uniform,
                encode: encode.clone(),
                decode: decode.clone(),
                encoded_pipelines: default(),
                linear_fallback_draws: 0,
            }
        });
        if view.rect != rect {
            view.rect = rect;
            view.uniform.set(rect);
            view.uniform.write_buffer(&device, &queue);
        }
        // Hold the compiled pipelines across queue/render. A late cache reload
        // cannot leave already-encoded queued draws without their decoder.
        view.encode = encode.clone();
        view.decode = decode.clone();
        view.encoded_pipelines.clear();
        view.linear_fallback_draws = 0;
    }
}

fn render_encoded(
    world: &World,
    view: ViewQuery<(
        &ExtractedCamera,
        &ExtractedView,
        &ViewTarget,
        &ViewDepthTexture,
        Option<&CompositingFeedback>,
    )>,
    prepared: Res<CompositingViews>,
    pipelines: Res<TransferPipelines>,
    cache: Res<PipelineCache>,
    phases: Res<ViewSortedRenderPhases<Transparent3d>>,
    mut context: RenderContext,
) -> bool {
    let entity = view.entity();
    let Some(prepared) = prepared.0.get(&entity) else {
        return false;
    };
    let (camera, extracted, target, depth, feedback) = view.into_inner();
    let report = |state| {
        if let Some(feedback) = feedback {
            feedback.publish(state);
        }
    };
    if prepared.encoded_pipelines.is_empty() {
        report(if prepared.linear_fallback_draws > 0 {
            CompositingStatus::LinearFallback(CompositingFallback::GaussianPipeline)
        } else {
            CompositingStatus::NoVisibleDraws
        });
        return false;
    }
    let Some(phase) = phases.get(&extracted.retained_view_entity) else {
        report(CompositingStatus::NoVisibleDraws);
        return false;
    };
    let Some(binding) = prepared.uniform.binding() else {
        report(CompositingStatus::Error);
        return false;
    };
    let layout = cache.get_bind_group_layout(&pipelines.layout);
    // Prepare both sources before flipping the target. Other earlier passes
    // may have swapped A/B, so match the actual view IDs, not fixed parity.
    let groups = [target.main_texture_view(), target.main_texture_other_view()].map(|source| {
        (
            source.id(),
            context.render_device().create_bind_group(
                "Gaussian colour transfer",
                &layout,
                &BindGroupEntries::sequential((source, binding.clone())),
            ),
        )
    });
    let mut start = 0;
    let mut runs = 0_u32;
    let mut draw_error = false;
    while start < phase.items.len() {
        let encoded = prepared
            .encoded_pipelines
            .contains(&phase.items[start].pipeline);
        let mut end = start;
        while end < phase.items.len() {
            let item = &phase.items[end];
            if !item.batch_range.is_empty()
                && prepared.encoded_pipelines.contains(&item.pipeline) != encoded
            {
                break;
            }
            end = end
                .saturating_add(item.batch_range.len().max(1))
                .min(phase.items.len());
        }
        if end == start {
            end += 1;
        }
        if encoded {
            runs = runs.saturating_add(1);
            transfer(&mut context, target, &prepared.encode, &groups);
        }
        {
            let mut pass = context.begin_tracked_render_pass(RenderPassDescriptor {
                label: Some("Gaussian ordered transparency segment"),
                color_attachments: &[Some(target.get_color_attachment())],
                depth_stencil_attachment: Some(depth.get_attachment(StoreOp::Store)),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(viewport) = camera.viewport.as_ref() {
                pass.set_camera_viewport(viewport);
            }
            if let Err(error) = phase.render_range(&mut pass, world, entity, start..end) {
                draw_error = true;
                error!("Error rendering ordered Gaussian transparency: {error:?}");
            }
        }
        if encoded {
            transfer(&mut context, target, &prepared.decode, &groups);
        }
        start = end;
    }
    report(if draw_error {
        CompositingStatus::Error
    } else if runs == 0 {
        CompositingStatus::NoVisibleDraws
    } else {
        CompositingStatus::Encoded {
            runs,
            linear_fallback_draws: prepared.linear_fallback_draws,
            transfer_texel_bytes: u64::from(target.main_texture().width())
                .saturating_mul(u64::from(target.main_texture().height()))
                .saturating_mul(32)
                .saturating_mul(u64::from(runs)),
        }
    });
    true
}

fn transfer(
    context: &mut RenderContext,
    target: &ViewTarget,
    pipeline: &RenderPipeline,
    groups: &[(TextureViewId, BindGroup); 2],
) {
    // These are the same two views captured immediately above, with no target
    // mutation between segments except our paired flips.
    let group = if groups[0].0 == target.main_texture_view().id() {
        &groups[0].1
    } else {
        &groups[1].1
    };
    let write = target.post_process_write();
    let mut pass = context
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("Gaussian colour transfer"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: write.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, group, &[]);
    pass.draw(0..3, 0..1);
}
