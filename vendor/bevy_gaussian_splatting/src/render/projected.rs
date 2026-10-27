//! Opt-in per-view projection cache. One record per selected source index keeps
//! deterministic identity; radix stably compacts its survivor index stream.
use super::{compact::CompactCloud, *};
use bevy::render::{
    extract_component::DynamicUniformIndex,
    renderer::{RenderContext, ViewQuery},
};

pub const RECORD_BYTES: u64 = 32;
pub const SHADER: Handle<Shader> = uuid_handle!("9fa221b7-e737-4abc-b6f6-97c26fb9d539");

#[derive(Clone)]
pub(crate) struct ProjectionCache {
    pub buffer: Buffer,
    pub ready: bool,
}

pub fn layout_descriptor() -> BindGroupLayoutDescriptor {
    BindGroupLayoutDescriptor::new(
        "projected SOG records",
        &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::VERTEX | ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: BufferSize::new(RECORD_BYTES),
            },
            count: None,
        }],
    )
}

/// Unsupported appearance modes keep the ordinary compact vertex path.
pub fn supports(settings: &CloudSettings) -> bool {
    cfg!(feature = "io_sog")
        && super::compact::supports(settings)
        && !settings.aabb
        && settings.raster_profile == crate::GaussianRasterProfile::NormalizedGaussian
        && settings.rasterize_mode == crate::RasterizeMode::Color
        && settings.draw_mode == DrawMode::All
}

#[derive(Resource)]
/// Render-world projector state. Compilation is requested lazily by eligible work.
pub struct Projector {
    pipeline: Option<CachedComputePipelineId>,
    descriptor: ComputePipelineDescriptor,
    input: BindGroupLayout,
    output: BindGroupLayout,
}

impl Projector {
    /// Permanent compilation failures require the ordinary vertex fallback.
    /// Missing imports/assets are transient; Bevy retries those automatically.
    /// Shader reload can recover a failed pipeline without changing its ID.
    pub fn has_failed(&self, cache: &PipelineCache) -> bool {
        use bevy::shader::ShaderCacheError;
        self.pipeline.is_some_and(|id| {
            matches!(
                cache.get_compute_pipeline_state(id),
                CachedPipelineState::Err(
                    ShaderCacheError::ProcessShaderError(_)
                        | ShaderCacheError::CreateShaderModule(_)
                )
            )
        })
    }
}

impl FromWorld for Projector {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let cloud = world.resource::<CloudPipeline<crate::Gaussian3d>>();
        let input_desc = super::compact::layout_descriptor(false);
        let output_desc = BindGroupLayoutDescriptor::new(
            "projected SOG output",
            &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(RECORD_BYTES),
                },
                count: None,
            }],
        );
        let input = device.create_bind_group_layout(Some("projection input"), &input_desc.entries);
        let output =
            device.create_bind_group_layout(Some("projection output"), &output_desc.entries);
        let descriptor = ComputePipelineDescriptor {
            label: Some("SOG compute projection".into()),
            layout: vec![
                cloud.compute_view_layout_desc.clone(),
                cloud.gaussian_uniform_layout_desc.clone(),
                input_desc,
                output_desc,
            ],
            shader: SHADER,
            shader_defs: shader_defs(CloudPipelineKey {
                compact_work: true,
                raster_profile: crate::GaussianRasterProfile::NormalizedGaussian,
                ..default()
            }),
            entry_point: Some("project".into()),
            ..default()
        };
        Self {
            pipeline: None,
            descriptor,
            input,
            output,
        }
    }
}

/// Runs after packed SH baking and before sort. Reprojection currently happens
/// each rendered frame; no camera/appearance invalidation can reuse stale data.
pub(crate) fn project_view(
    mut context: RenderContext,
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    mut projector: ResMut<Projector>,
    uniforms: Res<GaussianUniformBindGroups>,
    view: ViewQuery<(
        &GaussianComputeViewBindGroup,
        &ViewUniformOffset,
        &PreviousViewUniformOffset,
    )>,
    mut clouds: Query<(
        &mut CompactCloud,
        &CloudSettings,
        &DynamicUniformIndex<CloudUniform>,
    )>,
) {
    let owner = view.entity();
    let (view, view_index, previous_index) = view.into_inner();
    for (mut cloud, settings, index) in &mut clouds {
        if cloud.view() != owner || cloud.projected.is_none() {
            continue;
        }
        if let Some(output) = cloud.projected.as_mut() {
            output.ready = false;
        }
        if !cloud.ready || !supports(settings) {
            continue;
        }
        // Opting in must be the only way to compile this experimental shader.
        // Retain the ID across generations/views so toggling does not enqueue
        // duplicates. Pending clouds still reach this stage before activation.
        let id = match projector.pipeline {
            Some(id) => id,
            None => {
                let id = cache.queue_compute_pipeline(projector.descriptor.clone());
                projector.pipeline = Some(id);
                id
            }
        };
        let (Some(pipeline), Some(uniforms)) = (
            cache.get_compute_pipeline(id),
            uniforms.base_bind_group.as_ref(),
        ) else {
            continue;
        };
        let input = cloud.work_bind_group(&device, &projector.input);
        let Some(output) = cloud.projected.as_ref() else {
            continue;
        };
        let output_group = device.create_bind_group(
            "projected output",
            &projector.output,
            &[BindGroupEntry {
                binding: 0,
                resource: output.buffer.as_entire_binding(),
            }],
        );
        let groups = (cloud.len() as u32).div_ceil(64);
        // A two-dimensional dispatch covers selections beyond baseline 65535 workgroups.
        let x = groups.min(device.limits().max_compute_workgroups_per_dimension);
        if x == 0 {
            continue;
        }
        let y = groups.div_ceil(x);
        if y > device.limits().max_compute_workgroups_per_dimension {
            continue;
        }
        {
            let mut pass = context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor {
                    label: Some("SOG project once"),
                    timestamp_writes: None,
                });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &view.value, &[view_index.offset, previous_index.offset]);
            pass.set_bind_group(1, uniforms, &[index.index()]);
            pass.set_bind_group(2, &input, &[]);
            pass.set_bind_group(3, &output_group, &[]);
            pass.dispatch_workgroups(x, y, 1);
        }
        if let Some(output) = cloud.projected.as_mut() {
            output.ready = true;
        }
    }
}
