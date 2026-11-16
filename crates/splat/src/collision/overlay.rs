//! Screen-space voxel collision overlay, matching SuperSplat's debug view.
//!
//! A fullscreen pass ray-marches the octree per pixel (block DDA with empty-cell
//! skipping, then voxel DDA inside non-empty blocks) and blends translucent,
//! edge-outlined voxel faces over the splats. The grid stays in the scene's
//! source frame; each ray is transformed into it, so orientation changes on the
//! splat root apply without re-uploading data. Storage buffers make this
//! WebGPU/native only.
use std::sync::Arc;

use bevy::{
    asset::{embedded_asset, load_embedded_asset},
    core_pipeline::{
        FullscreenShader,
        schedule::{Core3d, Core3dSystems},
        tonemapping::tonemapping,
    },
    ecs::error::BevyError,
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderStartup, RenderSystems,
        camera::ExtractedCamera,
        render_resource::{
            BindGroup, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries,
            BlendState, Buffer, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId,
            Canonical, ColorTargetState, ColorWrites, FragmentState, PipelineCache,
            RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor, ShaderStages,
            ShaderType, Specializer, SpecializerKey, TextureFormat, UniformBuffer, Variants,
            binding_types::{storage_buffer_read_only_sized, uniform_buffer},
        },
        renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery},
        view::{ExtractedView, ViewTarget},
    },
};

use super::{VoxelCollision, VoxelOverlay};

/// Registers the overlay pass; it draws nothing until [`VoxelOverlay`] enables it.
pub struct VoxelOverlayPlugin;
impl Plugin for VoxelOverlayPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "overlay.wgsl");
        app.init_resource::<VoxelOverlay>();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(ExtractSchedule, extract)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareBindGroups))
            .add_systems(
                Core3d,
                draw.in_set(Core3dSystems::PostProcess).before(tonemapping),
            );
    }
}

#[derive(Resource)]
struct ExtractedOverlay {
    collision: Arc<VoxelCollision>,
    world_from_source: Mat4,
}

fn extract(
    mut commands: Commands,
    overlay: Extract<Res<VoxelOverlay>>,
    transforms: Extract<Query<&GlobalTransform>>,
) {
    match &overlay.collision {
        Some(collision) if overlay.enabled && !collision.nodes.is_empty() => {
            let world_from_source = overlay
                .root
                .and_then(|root| transforms.get(root).ok())
                .map_or(Mat4::IDENTITY, GlobalTransform::to_matrix);
            commands.insert_resource(ExtractedOverlay {
                collision: collision.clone(),
                world_from_source,
            });
        }
        _ => commands.remove_resource::<ExtractedOverlay>(),
    }
}

#[derive(ShaderType)]
struct Uniforms {
    /// Rays are built in view space, where the near plane (as close as 0.1 mm
    /// in the viewer) does not cancel against world-scale coordinates.
    view_from_clip: Mat4,
    grid_from_view: Mat4,
    viewport: Vec4,
    eye: Vec3,
    /// Pixel size in grid units: per unit distance (perspective) or absolute
    /// (orthographic).
    pixel_scale: f32,
    grid_min: Vec3,
    voxel_res: f32,
    voxels: UVec3,
    tree_depth: u32,
    inverted: u32,
    orthographic: u32,
}

#[derive(Resource)]
struct OverlayPipeline {
    layout: BindGroupLayoutDescriptor,
    variants: Variants<RenderPipeline, OverlaySpecializer>,
}

struct OverlaySpecializer;

#[derive(PartialEq, Eq, Hash, Clone, Copy, SpecializerKey)]
struct OverlayKey {
    target_format: TextureFormat,
}

impl Specializer<RenderPipeline> for OverlaySpecializer {
    type Key = OverlayKey;

    fn specialize(
        &self,
        key: Self::Key,
        descriptor: &mut RenderPipelineDescriptor,
    ) -> Result<Canonical<Self::Key>, BevyError> {
        descriptor.fragment_mut()?.set_target(
            0,
            ColorTargetState {
                format: key.target_format,
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            },
        );
        Ok(key)
    }
}

fn init_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let layout = BindGroupLayoutDescriptor::new(
        "voxel_overlay_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                uniform_buffer::<Uniforms>(false),
                storage_buffer_read_only_sized(false, None),
                storage_buffer_read_only_sized(false, None),
            ),
        ),
    );
    let descriptor = RenderPipelineDescriptor {
        label: Some("voxel_overlay_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader: load_embedded_asset!(asset_server.as_ref(), "overlay.wgsl"),
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };
    commands.insert_resource(OverlayPipeline {
        layout,
        variants: Variants::new(OverlaySpecializer, descriptor),
    });
}

/// Octree buffers, re-uploaded only when the collision data changes.
#[derive(Resource, Clone)]
struct OverlayBuffers {
    source: Arc<VoxelCollision>,
    nodes: Buffer,
    leaf_data: Buffer,
}

#[derive(Component)]
struct OverlayView {
    pipeline: CachedRenderPipelineId,
    bind_group: BindGroup,
}

fn storage(device: &RenderDevice, label: &'static str, words: &[u32]) -> Buffer {
    // Empty storage bindings are invalid; keep one zero word.
    let words = if words.is_empty() { &[0][..] } else { words };
    device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(words),
        usage: BufferUsages::STORAGE,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    mut commands: Commands,
    overlay: Option<Res<ExtractedOverlay>>,
    buffers: Option<Res<OverlayBuffers>>,
    pipeline: Option<ResMut<OverlayPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    views: Query<(Entity, &ExtractedView, Has<OverlayView>), With<ExtractedCamera>>,
) -> Result<(), BevyError> {
    let (Some(overlay), Some(mut pipeline)) = (overlay, pipeline) else {
        for (entity, _, drawn) in &views {
            if drawn {
                commands.entity(entity).remove::<OverlayView>();
            }
        }
        if buffers.is_some() {
            commands.remove_resource::<OverlayBuffers>();
        }
        return Ok(());
    };
    let collision = &overlay.collision;
    let buffers = match buffers.filter(|b| Arc::ptr_eq(&b.source, collision)) {
        Some(buffers) => buffers.clone(),
        None => {
            let buffers = OverlayBuffers {
                source: collision.clone(),
                nodes: storage(&device, "voxel_overlay_nodes", &collision.nodes),
                leaf_data: storage(&device, "voxel_overlay_leaves", &collision.leaf_data),
            };
            commands.insert_resource(buffers.clone());
            buffers
        }
    };
    let grid_from_world = collision.grid_from_source() * overlay.world_from_source.inverse();
    // Pixel footprints scale with the root's (uniform) scale.
    let grid_per_world = grid_from_world.x_axis.truncate().length();
    let layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);
    for (entity, view, _) in &views {
        let world_from_view = view.world_from_view.to_matrix();
        let eye = grid_from_world.transform_point3(view.world_from_view.translation());
        let size = view.viewport.zw().max(UVec2::ONE).as_vec2();
        let mut uniforms = UniformBuffer::from(Uniforms {
            view_from_clip: view.clip_from_view.inverse(),
            grid_from_view: grid_from_world * world_from_view,
            viewport: view.viewport.as_vec4(),
            eye,
            pixel_scale: 2.0 * grid_per_world / (size.y * view.clip_from_view.y_axis.y),
            grid_min: collision.grid_min,
            voxel_res: collision.resolution,
            voxels: collision.voxels,
            tree_depth: collision.tree_depth,
            inverted: collision.is_solid_at(eye) as u32,
            orthographic: (view.clip_from_view.w_axis.w == 1.0) as u32,
        });
        uniforms.write_buffer(&device, &queue);
        let Some(uniform) = uniforms.binding() else {
            continue;
        };
        let bind_group = device.create_bind_group(
            "voxel_overlay_bind_group",
            &layout,
            &BindGroupEntries::sequential((
                uniform,
                buffers.nodes.as_entire_binding(),
                buffers.leaf_data.as_entire_binding(),
            )),
        );
        let id = pipeline.variants.specialize(
            &pipeline_cache,
            OverlayKey {
                target_format: view.target_format,
            },
        )?;
        commands.entity(entity).insert(OverlayView {
            pipeline: id,
            bind_group,
        });
    }
    Ok(())
}

fn draw(
    view: ViewQuery<(&ViewTarget, &ExtractedCamera, &OverlayView)>,
    pipeline_cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (target, camera, overlay) = view.into_inner();
    let Some(pipeline) = pipeline_cache.get_render_pipeline(overlay.pipeline) else {
        return;
    };
    let mut pass = ctx.begin_tracked_render_pass(RenderPassDescriptor {
        label: Some("voxel_overlay_pass"),
        color_attachments: &[Some(target.get_color_attachment())],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    if let Some(viewport) = camera.viewport.as_ref() {
        pass.set_camera_viewport(viewport);
    }
    pass.set_render_pipeline(pipeline);
    pass.set_bind_group(0, &overlay.bind_group, &[]);
    pass.draw(0..3, 0..1);
}
