use super::*;
use bevy::{
    asset::{load_internal_asset, uuid_handle},
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    ecs::system::{SystemParamItem, lifetimeless::*},
    mesh::VertexBufferLayout,
    render::{
        Render, RenderStartup, RenderSystems,
        render_phase::{
            AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand,
            RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases,
        },
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        sync_world::MainEntity,
        view::{ExtractedView, ViewUniform, ViewUniformOffset, ViewUniforms},
    },
};
use std::collections::HashMap;

const SHADER: Handle<Shader> = uuid_handle!("2ae9a550-25ef-4c82-b775-c36e42a7edca");

/// Register the standalone transparent pipeline and bounded per-view buffers.
pub(super) fn install(app: &mut App, ready: ReadyClouds) {
    load_internal_asset!(app, SHADER, "shader.wgsl", Shader::from_wgsl);
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(ready)
            .init_resource::<ExtractedClouds>()
            .init_resource::<Buffers>()
            .init_resource::<Sources>()
            .init_resource::<ViewBinding>()
            .init_resource::<SpecializedRenderPipelines<Pipeline>>()
            .add_render_command::<Transparent3d, Draw>()
            .add_systems(RenderStartup, init_pipeline)
            .add_systems(Render, prepare_and_queue.in_set(RenderSystems::Queue))
            .add_systems(
                Render,
                prepare_binding.in_set(RenderSystems::PrepareBindGroups),
            );
    }
}

#[derive(Resource)]
struct Pipeline {
    layout: BindGroupLayout,
    descriptor: BindGroupLayoutDescriptor,
    attributes: BindGroupLayout,
    attributes_descriptor: BindGroupLayoutDescriptor,
}

fn init_pipeline(mut commands: Commands, device: Res<RenderDevice>) {
    let entries = [BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: Some(ViewUniform::min_size()),
        },
        count: None,
    }];
    let attributes = [BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable: false },
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }];
    commands.insert_resource(Pipeline {
        attributes: device.create_bind_group_layout(Some("gaussian_webgl_attributes"), &attributes),
        attributes_descriptor: BindGroupLayoutDescriptor::new(
            "gaussian_webgl_attributes",
            &attributes,
        ),
        layout: device.create_bind_group_layout(Some("gaussian_webgl_view"), &entries),
        descriptor: BindGroupLayoutDescriptor::new("gaussian_webgl_view", &entries),
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    format: TextureFormat,
    samples: u32,
    normalized: bool,
    additive: bool,
}

impl SpecializedRenderPipeline for Pipeline {
    type Key = Key;
    fn specialize(&self, key: Key) -> RenderPipelineDescriptor {
        let defs = if key.normalized {
            vec!["NORMALIZED_GAUSSIAN".into()]
        } else {
            vec![]
        };
        RenderPipelineDescriptor {
            label: Some("gaussian_webgl".into()),
            layout: vec![self.descriptor.clone(), self.attributes_descriptor.clone()],
            vertex: VertexState {
                shader: SHADER,
                shader_defs: defs.clone(),
                entry_point: Some("vertex".into()),
                buffers: vec![VertexBufferLayout {
                    array_stride: 4,
                    step_mode: VertexStepMode::Instance,
                    attributes: vec![VertexAttribute {
                        format: VertexFormat::Uint32,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            fragment: Some(FragmentState {
                shader: SHADER,
                shader_defs: defs,
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState {
                    format: key.format,
                    write_mask: ColorWrites::ALL,
                    blend: Some(if key.additive {
                        BlendState {
                            color: BlendComponent {
                                src_factor: BlendFactor::One,
                                dst_factor: BlendFactor::One,
                                operation: BlendOperation::Add,
                            },
                            alpha: BlendComponent {
                                src_factor: BlendFactor::One,
                                dst_factor: BlendFactor::One,
                                operation: BlendOperation::Add,
                            },
                        }
                    } else {
                        BlendState::PREMULTIPLIED_ALPHA_BLENDING
                    }),
                })],
            }),
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: default(),
                bias: default(),
            }),
            multisample: MultisampleState {
                count: key.samples,
                ..default()
            },
            ..default()
        }
    }
}

// Attribute textures are source-scoped and shared across views. A displayed
// generation retains its Arc until the replacement's first order is complete.
struct Source {
    cloud: CpuCloud,
    _texture: Texture,
    binding: BindGroup,
}
#[derive(Resource, Default)]
struct Sources(HashMap<Entity, Arc<Source>>);
struct Displayed {
    source: Arc<Source>,
    indices: Buffer,
}
struct CloudBuffer {
    entity: Entity,
    incoming: Arc<Source>,
    ordering: super::ordering::Ordering,
    displayed: Option<Displayed>,
}
#[derive(Resource, Default)]
struct Buffers(HashMap<(Entity, Entity), CloudBuffer>);
#[derive(Resource, Default)]
struct ViewBinding(Option<BindGroup>);

fn source_texture(
    cloud: &CpuCloud,
    device: &RenderDevice,
    queue: &RenderQueue,
    pipeline: &Pipeline,
) -> Source {
    let (width, height) =
        attribute_extent(cloud.data.len(), device.limits().max_texture_dimension_2d)
            .expect("WebGL source admitted to texture limits");
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("gaussian_webgl_attributes"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba32Float,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Complete rows allow a single upload; the padding is bounded by one row.
    let mut texels = vec![0u8; width as usize * height as usize * 16];
    let bytes = bytemuck::cast_slice(cloud.data.as_slice());
    texels[..bytes.len()].copy_from_slice(bytes);
    queue.write_texture(
        texture.as_image_copy(),
        &texels,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&TextureViewDescriptor::default());
    let binding = device.create_bind_group(
        "gaussian_webgl_attributes",
        &pipeline.attributes,
        &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&view),
        }],
    );
    Source {
        cloud: cloud.clone(),
        _texture: texture,
        binding,
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_and_queue(
    mut commands: Commands,
    clouds: Res<ExtractedClouds>,
    views: Query<(Entity, &ExtractedView, &Msaa), With<crate::GaussianCamera>>,
    mut buffers: ResMut<Buffers>,
    mut sources: ResMut<Sources>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    pipeline: Res<Pipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<Pipeline>>,
    cache: Res<PipelineCache>,
    draw: Res<DrawFunctions<Transparent3d>>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    ready: Res<ReadyClouds>,
) {
    let mut live = HashSet::new();
    let mut prepared_views = Vec::new();
    let live_sources: HashSet<_> = clouds.0.iter().map(|(id, _, _)| *id).collect();
    sources.0.retain(|id, _| live_sources.contains(id));
    for (id, cloud, _) in &clouds.0 {
        if cloud.data.is_empty()
            || attribute_extent(cloud.data.len(), device.limits().max_texture_dimension_2d)
                .is_none()
        {
            sources.0.remove(id);
        } else if attribute_extent(cloud.data.len(), device.limits().max_texture_dimension_2d)
            .is_some()
            && !sources
                .0
                .get(id)
                .is_some_and(|old| Arc::ptr_eq(&old.cloud.data, &cloud.data))
        {
            sources.0.insert(
                *id,
                Arc::new(source_texture(cloud, &device, &queue, &pipeline)),
            );
        }
    }
    for (view_entity, view, msaa) in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let mut view_ready = HashSet::new();
        let view_from_world = view.world_from_view.to_matrix().inverse();
        for (main_entity, cloud, visible) in &clouds.0 {
            let id = pipelines.specialize(
                &cache,
                &pipeline,
                Key {
                    format: view.target_format,
                    samples: msaa.samples(),
                    normalized: cloud.normalized,
                    additive: cloud.additive,
                },
            );
            let key = (view_entity, *main_entity);
            live.insert(key);
            if cloud.data.is_empty() {
                if let Some(old) = buffers.0.remove(&key) {
                    commands.entity(old.entity).despawn();
                }
                if cache.get_render_pipeline(id).is_some() {
                    view_ready.insert(*main_entity);
                }
                continue;
            }
            let Some(source) = sources.0.get(main_entity) else {
                continue;
            };
            let buffer = buffers.0.entry(key).or_insert_with(|| CloudBuffer {
                entity: commands.spawn_empty().id(),
                incoming: source.clone(),
                ordering: super::ordering::Ordering::new(source.cloud.data.clone()),
                displayed: None,
            });
            if !Arc::ptr_eq(&buffer.incoming, source) {
                buffer.ordering = super::ordering::Ordering::new(source.cloud.data.clone());
                buffer.incoming = source.clone();
            }
            if let Some(indices) = buffer.ordering.update(view_from_world) {
                let bytes = bytemuck::cast_slice(&indices);
                if let Some(displayed) = &mut buffer.displayed {
                    if displayed.indices.size() == bytes.len() as u64 {
                        queue.write_buffer(&displayed.indices, 0, bytes);
                        displayed.source = source.clone();
                    } else {
                        buffer.displayed = None;
                    }
                }
                if buffer.displayed.is_none() {
                    buffer.displayed = Some(Displayed {
                        source: source.clone(),
                        indices: device.create_buffer_with_data(&BufferInitDescriptor {
                            label: Some("gaussian_webgl_order"),
                            contents: bytes,
                            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                        }),
                    });
                }
            }
            let Some(displayed) = &buffer.displayed else {
                continue;
            };
            if Arc::ptr_eq(&displayed.source, source) && cache.get_render_pipeline(id).is_some() {
                view_ready.insert(*main_entity);
            }
            if !visible {
                continue;
            }
            let displayed_cloud = &displayed.source.cloud;
            let display_pipeline = pipelines.specialize(
                &cache,
                &pipeline,
                Key {
                    format: view.target_format,
                    samples: msaa.samples(),
                    normalized: displayed_cloud.normalized,
                    additive: displayed_cloud.additive,
                },
            );
            phase.add_transient(Transparent3d {
                sorting_info: TransparentSortingInfo3d::Sorted {
                    mesh_center: displayed_cloud.center,
                    depth_bias: 0.0,
                },
                entity: (buffer.entity, MainEntity::from(*main_entity)),
                pipeline: display_pipeline,
                draw_function: draw.read().id::<Draw>(),
                distance: view.rangefinder3d().distance(&displayed_cloud.center),
                batch_range: 0..1,
                extra_index: PhaseItemExtraIndex::None,
                indexed: false,
            });
        }
        prepared_views.push(view_ready);
    }
    ready.publish_views(&prepared_views);
    buffers.0.retain(|key, buffer| {
        if live.contains(key) {
            true
        } else {
            commands.entity(buffer.entity).despawn();
            false
        }
    });
}

fn prepare_binding(
    mut binding: ResMut<ViewBinding>,
    uniforms: Res<ViewUniforms>,
    pipeline: Res<Pipeline>,
    device: Res<RenderDevice>,
) {
    binding.0 = uniforms.uniforms.binding().map(|resource| {
        device.create_bind_group(
            "gaussian_webgl_view",
            &pipeline.layout,
            &[BindGroupEntry {
                binding: 0,
                resource,
            }],
        )
    });
}

type Draw = (SetItemPipeline, DrawInstances);
struct DrawInstances;
impl<P: PhaseItem> RenderCommand<P> for DrawInstances {
    type Param = (SRes<Buffers>, SRes<ViewBinding>);
    type ViewQuery = (Entity, Read<ViewUniformOffset>);
    type ItemQuery = ();
    fn render<'w>(
        item: &P,
        (view, offset): (Entity, &'w ViewUniformOffset),
        _: Option<()>,
        (buffers, binding): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(buffer) = buffers.into_inner().0.get(&(view, item.main_entity().id())) else {
            return RenderCommandResult::Skip;
        };
        let Some(binding) = &binding.into_inner().0 else {
            return RenderCommandResult::Skip;
        };
        let Some(displayed) = &buffer.displayed else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(0, binding, &[offset.offset]);
        pass.set_bind_group(1, &displayed.source.binding, &[]);
        pass.set_vertex_buffer(0, displayed.indices.slice(..));
        pass.draw(0..4, 0..displayed.source.cloud.data.len() as u32);
        RenderCommandResult::Success
    }
}
