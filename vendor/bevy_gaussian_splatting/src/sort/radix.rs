#[cfg(feature = "morph_interpolate")]
use std::any::TypeId;
use std::collections::HashMap;

use bevy::{
    asset::{load_internal_asset, uuid_handle},
    core_pipeline::{Core3d, Core3dSystems, prepass::PreviousViewUniformOffset},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_component::DynamicUniformIndex,
        render_asset::RenderAssets,
        render_resource::{
            BindGroup, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
            BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBinding,
            BufferBindingType, BufferDescriptor, BufferInitDescriptor, BufferSize, BufferUsages,
            CachedComputePipelineId, CachedPipelineState, ComputePipelineDescriptor, PipelineCache,
            ShaderStages,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ViewUniformOffset,
    },
};
use bevy_interleave::prelude::*;
use static_assertions::assert_cfg;

#[cfg(feature = "render_diagnostics")]
use crate::diagnostics::{GaussianDiagnosticSample, GaussianRenderStatus, GaussianTimestampWrites};
#[cfg(feature = "render_diagnostics")]
use bevy::render::diagnostic::RecordDiagnostics;

#[cfg(feature = "morph_interpolate")]
use crate::{gaussian::formats::planar_3d::PlanarGaussian3d, morph::interpolate::InterpolateLabel};

use crate::{
    CloudSettings, GaussianCamera, RadixSortDepthBits,
    render::{
        CloudPipeline, CloudPipelineKey, CloudUniform, GaussianUniformBindGroups, ShaderDefines,
        compact::{self, CompactCloud},
        shader_defs_with_defines,
    },
    sort::{GpuSortedEntry, SortEntry, SortMode, SortPluginFlag, SortedEntriesHandle},
};

assert_cfg!(
    not(all(feature = "sort_radix", feature = "buffer_texture",)),
    "sort_radix and buffer_texture are incompatible",
);

const RADIX_SHADER_HANDLE: Handle<Shader> = uuid_handle!("dedb3ddf-f254-4361-8762-e221774de1ed");
const TEMPORAL_SORT_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("11986b71-25d8-410b-adfa-6afb107ae4de");
const RADIX_PIPELINE_RESET: usize = 0;
const RADIX_PIPELINE_A: usize = 1;
const RADIX_PIPELINE_B: usize = 2;
const RADIX_PIPELINE_C_COUNT: usize = 3;
const RADIX_PIPELINE_C_SCAN: usize = 4;
const RADIX_PIPELINE_C_SCATTER: usize = 5;
const COMPACT_COUNT: usize = 6;
const COMPACT_SCAN: usize = 7;
const COMPACT_SCATTER: usize = 8;
const RADIX_PIPELINE_COUNT: usize = 9;
const RADIX_DEPTH_BITS_VARIANT_COUNT: usize = 3;

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub struct RadixSortLabel;

#[derive(Default)]
pub struct RadixSortPlugin<R: PlanarSync> {
    phantom: std::marker::PhantomData<R>,
}

impl<R: PlanarSync> Plugin for RadixSortPlugin<R>
where
    R::GpuPlanarType: GpuPlanarStorage,
{
    fn build(&self, app: &mut App) {
        // TODO: run once
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                (queue_radix_bind_group::<R>.in_set(RenderSystems::Queue),),
            );

            render_app.init_resource::<RadixSortBuffers<R>>();
            render_app.add_systems(
                Render,
                update_sort_buffers::<R>
                    .after(RenderSystems::PrepareAssets)
                    .before(RenderSystems::Queue),
            );
        }

        if app.is_plugin_added::<SortPluginFlag>() {
            debug!("sort plugin already added");
            return;
        }

        load_internal_asset!(app, RADIX_SHADER_HANDLE, "radix.wgsl", Shader::from_wgsl);

        load_internal_asset!(
            app,
            TEMPORAL_SORT_SHADER_HANDLE,
            "temporal.wgsl",
            Shader::from_wgsl
        );

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            #[cfg(feature = "morph_interpolate")]
            if TypeId::of::<R::PlanarType>() == TypeId::of::<PlanarGaussian3d>() {
                render_app.add_systems(
                    Core3d,
                    run_radix_sort::<R>
                        .in_set(RadixSortLabel)
                        .after(InterpolateLabel)
                        .before(Core3dSystems::Prepass),
                );
            } else {
                render_app.add_systems(
                    Core3d,
                    run_radix_sort::<R>
                        .in_set(RadixSortLabel)
                        .before(Core3dSystems::Prepass),
                );
            }

            #[cfg(not(feature = "morph_interpolate"))]
            render_app.add_systems(
                Core3d,
                run_radix_sort::<R>
                    .in_set(RadixSortLabel)
                    .before(Core3dSystems::Prepass),
            );
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<RadixSortPipeline<R>>();
        }
    }
}

#[derive(Resource)]
pub struct RadixSortBuffers<R: PlanarSync> {
    // TODO: use a more ECS-friendly approach
    pub asset_map: HashMap<AssetId<R::PlanarType>, GpuRadixBuffers>,
    // Externally owned reusable slots can outlive all of their draw entities.
    persistent_capacities: HashMap<AssetId<R::PlanarType>, usize>,
}

impl<R: PlanarSync> Default for RadixSortBuffers<R> {
    fn default() -> Self {
        RadixSortBuffers {
            asset_map: HashMap::new(),
            persistent_capacities: HashMap::new(),
        }
    }
}

impl<R: PlanarSync> RadixSortBuffers<R> {
    /// Keep scratch for an externally owned slot, independent of draw entities.
    /// Call during synchronized extraction, before buffer preparation. The owner
    /// must reserve this capacity in its budget and call `release_persistent`
    /// when the slot retires. Changing capacity also permits reclaiming memory
    /// after a budget reduction; ordinary assets retain automatic lifetimes.
    pub fn retain_persistent(&mut self, id: AssetId<R::PlanarType>, capacity: usize) {
        if self.persistent_capacities.insert(id, capacity) != Some(capacity) {
            self.asset_map.remove(&id);
        }
    }

    /// Release an external slot and its scratch during synchronized extraction.
    pub fn release_persistent(&mut self, id: AssetId<R::PlanarType>) {
        self.persistent_capacities.remove(&id);
        self.asset_map.remove(&id);
    }
}

#[derive(Debug, Clone)]
pub struct GpuRadixBuffers {
    pub sorting_global_buffer: Buffer,
    pub sorting_status_counter_buffer: Buffer,
    pub sorting_pass_buffers: [Buffer; 4],
    pub entry_buffer_b: Buffer,
    /// Separate from writable storage bindings, as required by WebGPU usage scopes.
    pub dispatch_indirect_buffer: Buffer,
}

impl GpuRadixBuffers {
    pub fn new(count: usize, render_device: &RenderDevice) -> Self {
        let sorting_global_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("sorting global buffer"),
            size: ShaderDefines::default().sorting_buffer_size as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let sorting_status_counter_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("status counters buffer"),
            size: ShaderDefines::default().sorting_status_counters_buffer_size(count) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let sorting_pass_buffers = (0..4)
            .map(|idx| {
                render_device.create_buffer_with_data(&BufferInitDescriptor {
                    label: format!("sorting pass buffer {idx}").as_str().into(),
                    contents: &[idx as u8, 0, 0, 0],
                    usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                })
            })
            .collect::<Vec<Buffer>>()
            .try_into()
            .unwrap();

        let entry_buffer_b = render_device.create_buffer(&BufferDescriptor {
            label: Some("entry buffer b"),
            size: (count * std::mem::size_of::<SortEntry>()) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let dispatch_indirect_buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("radix visible dispatch"),
            size: 12,
            usage: BufferUsages::INDIRECT | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        GpuRadixBuffers {
            dispatch_indirect_buffer,
            sorting_global_buffer,
            sorting_status_counter_buffer,
            sorting_pass_buffers,
            entry_buffer_b,
        }
    }
}

fn update_sort_buffers<R: PlanarSync>(
    gpu_gaussian_clouds: Res<RenderAssets<R::GpuPlanarType>>,
    mut sort_buffers: ResMut<RadixSortBuffers<R>>,
    render_device: Res<RenderDevice>,
    compact_clouds: Query<(&R::PlanarTypeHandle, &CompactCloud)>,
) {
    let RadixSortBuffers {
        asset_map,
        persistent_capacities,
    } = &mut *sort_buffers;
    // Ordinary snapshots retire with their GPU asset/entity. Persistent slots
    // retire with their explicit owner, not a transient selection entity.
    asset_map.retain(|id, _| {
        persistent_capacities.contains_key(id)
            || gpu_gaussian_clouds.get(*id).is_some()
            || compact_clouds.iter().any(|(h, _)| h.handle().id() == *id)
    });
    for (asset_id, count) in persistent_capacities
        .iter()
        .map(|(id, count)| (*id, *count))
        .chain(
            gpu_gaussian_clouds
                .iter()
                .map(|(id, cloud)| (id, cloud.len()))
                .chain(
                    compact_clouds
                        .iter()
                        .map(|(h, c)| (h.handle().id(), c.len())),
                ),
        )
    {
        // Persistent GPU assets may grow under the same handle. Scratch must
        // cover the bound count before queueing new bind groups.
        if asset_map.get(&asset_id).is_some_and(|buffers| {
            buffers.entry_buffer_b.size() >= (count * std::mem::size_of::<SortEntry>()) as u64
                && buffers.sorting_status_counter_buffer.size()
                    >= ShaderDefines::default().sorting_status_counters_buffer_size(count) as u64
        }) {
            continue;
        }

        let gpu_radix_buffers = GpuRadixBuffers::new(count, &render_device);
        asset_map.insert(asset_id, gpu_radix_buffers);
    }
}

#[derive(Resource)]
pub struct RadixSortPipeline<R: PlanarSync> {
    pub radix_sort_layout: BindGroupLayout,
    pub radix_cloud_layout: BindGroupLayout,
    pub variants: [Option<RadixSortPipelineVariant>; RADIX_DEPTH_BITS_VARIANT_COUNT],
    pub compact_cloud_layout: BindGroupLayout,
    compact_variants: [Option<RadixSortPipelineVariant>; RADIX_DEPTH_BITS_VARIANT_COUNT],
    compact_sorting_layout: Vec<BindGroupLayoutDescriptor>,
    projected_cloud_layout: BindGroupLayout,
    projected_variants: [Option<RadixSortPipelineVariant>; RADIX_DEPTH_BITS_VARIANT_COUNT],
    projected_sorting_layout: Vec<BindGroupLayoutDescriptor>,
    sorting_layout: Vec<BindGroupLayoutDescriptor>,
    phantom: std::marker::PhantomData<R>,
}

#[derive(Clone, Copy)]
pub struct RadixSortPipelineVariant {
    pub shader_defines: ShaderDefines,
    pub radix_sort_pipelines: [Option<CachedComputePipelineId>; RADIX_PIPELINE_COUNT],
}

impl RadixSortPipelineVariant {
    fn required_pipelines(&self, compaction: bool) -> &[Option<CachedComputePipelineId>] {
        &self.radix_sort_pipelines[..if compaction {
            RADIX_PIPELINE_COUNT
        } else {
            COMPACT_COUNT
        }]
    }

    pub fn is_loaded(&self, pipeline_cache: &PipelineCache, compaction: bool) -> bool {
        self.required_pipelines(compaction)
            .iter()
            .all(|sort_pipeline| {
                sort_pipeline.is_some_and(|id| {
                    matches!(
                        pipeline_cache.get_compute_pipeline_state(id),
                        CachedPipelineState::Ok(_)
                    )
                })
            })
    }
}

impl<R: PlanarSync> RadixSortPipeline<R> {
    pub fn variant(
        &self,
        radix_sort_depth_bits: RadixSortDepthBits,
    ) -> Option<&RadixSortPipelineVariant> {
        self.variant_for_storage(radix_sort_depth_bits, false)
    }

    pub fn variant_for_storage(
        &self,
        bits: RadixSortDepthBits,
        compact: bool,
    ) -> Option<&RadixSortPipelineVariant> {
        self.variant_for_projection(bits, compact, false)
    }

    pub fn variant_for_projection(
        &self,
        bits: RadixSortDepthBits,
        compact: bool,
        projected: bool,
    ) -> Option<&RadixSortPipelineVariant> {
        let variants = if projected {
            &self.projected_variants
        } else if compact {
            &self.compact_variants
        } else {
            &self.variants
        };
        variants[bits.pipeline_index()].as_ref()
    }

    pub fn queue_variant(
        &mut self,
        pipeline_cache: &PipelineCache,
        radix_sort_depth_bits: RadixSortDepthBits,
        compaction: bool,
    ) {
        self.queue_storage_variant(pipeline_cache, radix_sort_depth_bits, compaction, false);
    }

    pub fn queue_storage_variant(
        &mut self,
        pipeline_cache: &PipelineCache,
        radix_sort_depth_bits: RadixSortDepthBits,
        compaction: bool,
        compact: bool,
    ) {
        self.queue_projection_variant(
            pipeline_cache,
            radix_sort_depth_bits,
            compaction,
            compact,
            false,
        );
    }

    pub fn queue_projection_variant(
        &mut self,
        pipeline_cache: &PipelineCache,
        radix_sort_depth_bits: RadixSortDepthBits,
        compaction: bool,
        compact: bool,
        projected: bool,
    ) {
        let index = radix_sort_depth_bits.pipeline_index();
        let variants = if projected {
            &mut self.projected_variants
        } else if compact {
            &mut self.compact_variants
        } else {
            &mut self.variants
        };
        let layout = if projected {
            &self.projected_sorting_layout
        } else if compact {
            &self.compact_sorting_layout
        } else {
            &self.sorting_layout
        };
        let variant = variants[index].get_or_insert_with(|| RadixSortPipelineVariant {
            shader_defines: ShaderDefines::for_radix_depth_bits(radix_sort_depth_bits),
            radix_sort_pipelines: [None; RADIX_PIPELINE_COUNT],
        });
        let stages = if compaction {
            RADIX_PIPELINE_COUNT
        } else {
            COMPACT_COUNT
        };
        for (stage, entry) in RADIX_ENTRYPOINTS.iter().enumerate().take(stages) {
            variant.radix_sort_pipelines[stage].get_or_insert_with(|| {
                pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
                    label: Some(format!("{entry}_{}bit", radix_sort_depth_bits.bits()).into()),
                    layout: layout.clone(),
                    immediate_size: 0,
                    shader: RADIX_SHADER_HANDLE,
                    shader_defs: shader_defs_with_defines(
                        CloudPipelineKey {
                            compact_work: compact,
                            projected_work: projected,
                            ..default()
                        },
                        variant.shader_defines,
                    ),
                    entry_point: Some((*entry).into()),
                    zero_initialize_workgroup_memory: true,
                })
            });
        }
    }
}

impl<R: PlanarSync> FromWorld for RadixSortPipeline<R> {
    fn from_world(render_world: &mut World) -> Self {
        let render_device = render_world.resource::<RenderDevice>();
        let gaussian_cloud_pipeline = render_world.resource::<CloudPipeline<R>>();

        let sorting_buffer_entry = BindGroupLayoutEntry {
            binding: 1,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: BufferSize::new(
                    ShaderDefines::default().sorting_buffer_size as u64,
                ),
            },
            count: None,
        };

        let sorting_status_counters_buffer_entry = BindGroupLayoutEntry {
            binding: 2,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: BufferSize::new(
                    ShaderDefines::default().sorting_status_counters_buffer_size(1) as u64,
                ),
            },
            count: None,
        };

        let draw_indirect_buffer_entry = BindGroupLayoutEntry {
            binding: 3,
            visibility: ShaderStages::COMPUTE,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: BufferSize::new(
                    std::mem::size_of::<wgpu::util::DrawIndirectArgs>() as u64,
                ),
            },
            count: None,
        };

        let radix_sort_layout_entries = [
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(std::mem::size_of::<u32>() as u64),
                },
                count: None,
            },
            sorting_buffer_entry,
            sorting_status_counters_buffer_entry,
            draw_indirect_buffer_entry,
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(std::mem::size_of::<SortEntry>() as u64),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(std::mem::size_of::<SortEntry>() as u64),
                },
                count: None,
            },
        ];
        let radix_sort_layout_desc =
            BindGroupLayoutDescriptor::new("radix_sort_layout", &radix_sort_layout_entries);
        let radix_sort_layout = render_device
            .create_bind_group_layout(Some("radix_sort_layout"), &radix_sort_layout_entries);

        // Radix reads only the position plane. The full draw layout exposes
        // every attribute to COMPUTE, exceeding WebGPU's eight-storage-buffer
        // baseline once the five sorting buffers are included. Keep the field
        // bindings so GpuPlanarStorage can build this group, but remove unused
        // planes from compute visibility. Drawing/morph layouts are unchanged.
        let mut cloud_entries = gaussian_cloud_pipeline
            .gaussian_cloud_layout_desc
            .entries
            .clone();
        for entry in &mut cloud_entries {
            if entry.binding != 0 {
                entry.visibility.remove(ShaderStages::COMPUTE);
            }
        }
        let radix_cloud_layout_desc =
            BindGroupLayoutDescriptor::new("radix_cloud_layout", &cloud_entries);
        let radix_cloud_layout =
            render_device.create_bind_group_layout(Some("radix_cloud_layout"), &cloud_entries);
        let sorting_layout = vec![
            gaussian_cloud_pipeline.compute_view_layout_desc.clone(),
            gaussian_cloud_pipeline.gaussian_uniform_layout_desc.clone(),
            radix_cloud_layout_desc,
            radix_sort_layout_desc.clone(),
        ];

        let variants = [None; RADIX_DEPTH_BITS_VARIANT_COUNT];
        let compact_descriptor = compact::layout_descriptor(true);
        let compact_cloud_layout = render_device
            .create_bind_group_layout(Some("compact radix cloud"), &compact_descriptor.entries);
        let mut compact_sorting_layout = sorting_layout.clone();
        compact_sorting_layout[2] = compact_descriptor;
        let projected_descriptor = crate::render::projected::layout_descriptor();
        let projected_cloud_layout = render_device
            .create_bind_group_layout(Some("projected radix cloud"), &projected_descriptor.entries);
        let mut projected_sorting_layout = sorting_layout.clone();
        projected_sorting_layout[2] = projected_descriptor;

        RadixSortPipeline {
            radix_sort_layout,
            radix_cloud_layout,
            variants,
            compact_cloud_layout,
            compact_variants: [None; RADIX_DEPTH_BITS_VARIANT_COUNT],
            compact_sorting_layout,
            projected_cloud_layout,
            projected_variants: [None; RADIX_DEPTH_BITS_VARIANT_COUNT],
            projected_sorting_layout,
            sorting_layout,
            phantom: std::marker::PhantomData,
        }
    }
}

const RADIX_ENTRYPOINTS: [&str; RADIX_PIPELINE_COUNT] = [
    "radix_reset",
    "radix_sort_a",
    "radix_sort_b",
    "radix_sort_c_count_tiles",
    "radix_sort_c_scan_tiles",
    "radix_sort_c_scatter",
    "compact_count",
    "compact_scan",
    "compact_scatter",
];

#[derive(Component)]
pub struct RadixBindGroup {
    pub cloud_bind_group: BindGroup,
    // For each digit pass idx in 0..RADIX_DIGIT_PLACES, we create 2 bind groups (parity 0/1):
    // index = pass_idx * 2 + parity (parity 0: input=sorted_entries, output=entry_buffer_b; parity 1: input=entry_buffer_b, output=sorted_entries)
    pub radix_sort_bind_groups: [BindGroup; 8],
}

#[cfg(not(feature = "render_diagnostics"))]
type RadixViewQueryItem = (
    &'static GaussianCamera,
    &'static crate::render::GaussianComputeViewBindGroup,
    &'static ViewUniformOffset,
    &'static PreviousViewUniformOffset,
);

#[cfg(feature = "render_diagnostics")]
type RadixViewQueryItem = (
    &'static GaussianCamera,
    &'static crate::render::GaussianComputeViewBindGroup,
    &'static ViewUniformOffset,
    &'static PreviousViewUniformOffset,
    Option<&'static GaussianDiagnosticSample>,
);

#[allow(clippy::too_many_arguments)]
pub fn queue_radix_bind_group<R: PlanarSync>(
    mut commands: Commands,
    mut radix_pipeline: ResMut<RadixSortPipeline<R>>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    gaussian_cloud_res: Res<RenderAssets<R::GpuPlanarType>>,
    sorted_entries_res: Res<RenderAssets<GpuSortedEntry>>,
    gaussian_clouds: Query<(
        Entity,
        &R::PlanarTypeHandle,
        &SortedEntriesHandle,
        &CloudSettings,
        Option<&CompactCloud>,
    )>,
    sort_buffers: Res<RadixSortBuffers<R>>,
) where
    R::GpuPlanarType: GpuPlanarStorage,
{
    for (entity, cloud_handle, sorted_entries_handle, settings, compact) in gaussian_clouds.iter() {
        if compact.is_some() && !compact::supports(settings) {
            continue;
        }
        if settings.sort_mode != SortMode::Radix {
            commands.entity(entity).remove::<RadixBindGroup>();
            continue;
        }

        radix_pipeline.queue_projection_variant(
            &pipeline_cache,
            settings.radix_sort_depth_bits,
            settings.visibility_compaction,
            compact.is_some(),
            compact.is_some_and(CompactCloud::has_projection_cache),
        );

        // TODO: deduplicate asset load checks
        if let Some(load_state) = asset_server.get_load_state(cloud_handle.handle())
            && load_state.is_loading()
        {
            continue;
        }

        if compact.is_none() && gaussian_cloud_res.get(cloud_handle.handle()).is_none() {
            continue;
        }

        if let Some(load_state) = asset_server.get_load_state(&sorted_entries_handle.0)
            && load_state.is_loading()
        {
            continue;
        }

        if sorted_entries_res.get(sorted_entries_handle).is_none() {
            continue;
        }

        if !sort_buffers
            .asset_map
            .contains_key(&cloud_handle.handle().id())
        {
            continue;
        }

        let cloud = gaussian_cloud_res.get(cloud_handle.handle());
        let Some((count, indirect, cloud_bind_group)) = compact
            .map(|c| {
                (
                    c.len(),
                    c.draw_indirect_buffer(),
                    c.bind_group(
                        &render_device,
                        if c.has_projection_cache() {
                            &radix_pipeline.projected_cloud_layout
                        } else {
                            &radix_pipeline.compact_cloud_layout
                        },
                    ),
                )
            })
            .or_else(|| {
                cloud.map(|c| {
                    (
                        c.len(),
                        c.draw_indirect_buffer(),
                        c.bind_group(&render_device, &radix_pipeline.radix_cloud_layout),
                    )
                })
            })
        else {
            continue;
        };
        let sorted_entries = sorted_entries_res.get(sorted_entries_handle).unwrap();
        let sorting_assets = &sort_buffers.asset_map[&cloud_handle.handle().id()];

        let sorting_global_entry = BindGroupEntry {
            binding: 1,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &sorting_assets.sorting_global_buffer,
                offset: 0,
                size: BufferSize::new(sorting_assets.sorting_global_buffer.size()),
            }),
        };

        let sorting_status_counters_entry = BindGroupEntry {
            binding: 2,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &sorting_assets.sorting_status_counter_buffer,
                offset: 0,
                size: BufferSize::new(sorting_assets.sorting_status_counter_buffer.size()),
            }),
        };

        let draw_indirect_entry = BindGroupEntry {
            binding: 3,
            resource: BindingResource::Buffer(BufferBinding {
                buffer: indirect,
                offset: 0,
                size: BufferSize::new(indirect.size()),
            }),
        };

        let radix_sort_bind_groups: [BindGroup; 8] = {
            let mut groups: Vec<BindGroup> = Vec::with_capacity(8);
            for pass_idx in 0..4 {
                for parity in 0..=1 {
                    let (input_buf, output_buf) = if parity == 0 {
                        (
                            &sorted_entries.sorted_entry_buffer,
                            &sorting_assets.entry_buffer_b,
                        )
                    } else {
                        (
                            &sorting_assets.entry_buffer_b,
                            &sorted_entries.sorted_entry_buffer,
                        )
                    };

                    let group = render_device.create_bind_group(
                        format!("radix_sort_bind_group pass={pass_idx} parity={parity}").as_str(),
                        &radix_pipeline.radix_sort_layout,
                        &[
                            // sorting_pass_index (u32) == pass_idx regardless of parity
                            BindGroupEntry {
                                binding: 0,
                                resource: BindingResource::Buffer(BufferBinding {
                                    buffer: &sorting_assets.sorting_pass_buffers[pass_idx],
                                    offset: 0,
                                    size: BufferSize::new(std::mem::size_of::<u32>() as u64),
                                }),
                            },
                            sorting_global_entry.clone(),
                            sorting_status_counters_entry.clone(),
                            draw_indirect_entry.clone(),
                            // input_entries
                            BindGroupEntry {
                                binding: 4,
                                resource: BindingResource::Buffer(BufferBinding {
                                    buffer: input_buf,
                                    offset: 0,
                                    size: BufferSize::new(
                                        (count * std::mem::size_of::<SortEntry>()) as u64,
                                    ),
                                }),
                            },
                            // output_entries
                            BindGroupEntry {
                                binding: 5,
                                resource: BindingResource::Buffer(BufferBinding {
                                    buffer: output_buf,
                                    offset: 0,
                                    size: BufferSize::new(
                                        (count * std::mem::size_of::<SortEntry>()) as u64,
                                    ),
                                }),
                            },
                        ],
                    );
                    groups.push(group);
                }
            }
            groups.try_into().unwrap()
        };

        commands.entity(entity).insert(RadixBindGroup {
            cloud_bind_group,
            radix_sort_bind_groups,
        });
    }
}

type RadixCloudQueryItem<R> = (
    Entity,
    &'static <R as PlanarSync>::PlanarTypeHandle,
    &'static RadixBindGroup,
    &'static DynamicUniformIndex<CloudUniform>,
    &'static CloudSettings,
    Option<&'static CompactCloud>,
);

#[allow(clippy::too_many_arguments)]
fn run_radix_sort<R: PlanarSync>(
    mut render_context: RenderContext,
    #[cfg(feature = "gpu_timing")] gpu_bridge: Option<Res<crate::gpu_timing::GaussianGpuTimings>>,
    #[cfg(feature = "gpu_timing")] gpu_recorder: Option<Res<crate::gpu_timing::Recorder>>,
    #[cfg(feature = "gpu_timing")] timing_sources: Query<&crate::gpu_timing::GaussianTimingSource>,
    #[cfg(feature = "gpu_timing")] timing_views: Query<
        (),
        With<crate::gpu_timing::GaussianTimingView>,
    >,
    #[cfg(feature = "render_diagnostics")] status: Res<GaussianRenderStatus>,
    #[cfg(feature = "render_diagnostics")] timestamps: Option<Res<GaussianTimestampWrites>>,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Res<RadixSortPipeline<R>>,
    gaussian_uniforms: Res<GaussianUniformBindGroups>,
    sort_buffers: Res<RadixSortBuffers<R>>,
    gpu_planars: Res<RenderAssets<R::GpuPlanarType>>,
    view_bind_group: ViewQuery<RadixViewQueryItem>,
    gaussian_clouds: Query<RadixCloudQueryItem<R>>,
) where
    R::GpuPlanarType: GpuPlanarStorage,
{
    let view_entity = view_bind_group.entity();
    #[cfg(not(feature = "render_diagnostics"))]
    let (_camera, view_bind_group, view_uniform_offset, previous_view_uniform_offset) =
        view_bind_group.into_inner();
    #[cfg(feature = "render_diagnostics")]
    let (_camera, view_bind_group, view_uniform_offset, previous_view_uniform_offset, sample) =
        view_bind_group.into_inner();

    let Some(uniform_bind_group) = gaussian_uniforms.base_bind_group.as_ref() else {
        debug!("RadixSort run skipped: GaussianUniform base bind group missing");
        return;
    };

    for (
        cloud_entity,
        cloud_handle,
        radix_bind_group,
        cloud_uniform_index,
        cloud_settings,
        compact,
    ) in &gaussian_clouds
    {
        if cloud_settings.stochastic() {
            continue;
        }
        if compact.is_some_and(|c| {
            c.view() != view_entity
                || !c.ready
                || !c.projection_ready()
                || !compact::supports(cloud_settings)
        }) {
            continue;
        }
        let cloud = gpu_planars.get(cloud_handle.handle());
        let Some((count, indirect)) = compact
            .map(|c| (c.len(), c.draw_indirect_buffer()))
            .or_else(|| cloud.map(|c| (c.len(), c.draw_indirect_buffer())))
        else {
            continue;
        };

        let Some(sorting_assets) = sort_buffers.asset_map.get(&cloud_handle.handle().id()) else {
            continue;
        };

        let Some(pipeline_variant) = pipeline.variant_for_projection(
            cloud_settings.radix_sort_depth_bits,
            compact.is_some(),
            compact.is_some_and(CompactCloud::has_projection_cache),
        ) else {
            continue;
        };
        if !pipeline_variant.is_loaded(&pipeline_cache, cloud_settings.visibility_compaction) {
            continue;
        }

        // The encoder never uses compaction stages when the flag is off. Give
        // those unused array positions the already-ready reset stage, without
        // requiring their creation or unwrapping a failed optional pipeline.
        let pipelines = std::array::from_fn(|index| {
            let stage = if !cloud_settings.visibility_compaction && index >= COMPACT_COUNT {
                RADIX_PIPELINE_RESET
            } else {
                index
            };
            let id = pipeline_variant.radix_sort_pipelines[stage].unwrap();
            &**pipeline_cache.get_compute_pipeline(id).unwrap()
        });
        let sort_groups = radix_bind_group
            .radix_sort_bind_groups
            .each_ref()
            .map(|g| &**g);
        #[cfg(feature = "render_diagnostics")]
        let recorder = render_context.diagnostic_recorder();
        #[cfg(feature = "render_diagnostics")]
        let recorder = recorder.as_deref();
        #[cfg(feature = "render_diagnostics")]
        let span = recorder.time_span(render_context.command_encoder(), "gaussian_radix");
        #[cfg(feature = "render_diagnostics")]
        let timestamps = timestamps
            .as_ref()
            .filter(|_| gaussian_clouds.iter().count() == 1)
            .and_then(|hooks| hooks.0.get(&view_entity));
        #[cfg(all(feature = "gpu_timing", not(feature = "render_diagnostics")))]
        let timestamps: Option<&crate::timestamps::GaussianTimestampInterval> = None;
        #[cfg(feature = "gpu_timing")]
        let gpu_interval = if timestamps.is_none() && timing_views.contains(view_entity) {
            gpu_bridge
                .as_deref()
                .zip(gpu_recorder.as_deref())
                .and_then(|(bridge, recorder)| {
                    let source = *timing_sources.get(cloud_entity).ok()?;
                    recorder.record(
                        bridge,
                        view_entity,
                        source,
                        count,
                        cloud_settings.visibility_compaction,
                    )
                })
        } else {
            None
        };
        #[cfg(feature = "gpu_timing")]
        let timestamps = timestamps.or(gpu_interval.as_ref());

        let command_encoder = render_context.command_encoder();
        encode_radix_sort(
            command_encoder,
            &pipelines,
            pipeline_variant.shader_defines,
            count as u32,
            cloud_settings.visibility_compaction,
            sorting_assets,
            indirect,
            [
                (
                    &view_bind_group.value,
                    &[
                        view_uniform_offset.offset,
                        previous_view_uniform_offset.offset,
                    ],
                ),
                (uniform_bind_group, &[cloud_uniform_index.index()]),
                (&radix_bind_group.cloud_bind_group, &[]),
            ],
            &sort_groups,
            #[cfg(any(feature = "render_diagnostics", feature = "gpu_timing"))]
            timestamps,
        );
        #[cfg(feature = "render_diagnostics")]
        {
            span.end(command_encoder);
            if let Some(sample) = sample {
                status.record_sort(view_entity, sample.0, cloud_entity);
            }
        }
        #[cfg(not(feature = "render_diagnostics"))]
        let _ = cloud_entity;
    }
}

/// Shared production encoder; the GPU regression calls this exact binding/dispatch path.
#[allow(clippy::too_many_arguments)]
fn encode_radix_sort(
    encoder: &mut wgpu::CommandEncoder,
    pipelines: &[&wgpu::ComputePipeline; RADIX_PIPELINE_COUNT],
    defs: ShaderDefines,
    count: u32,
    compact: bool,
    scratch: &GpuRadixBuffers,
    draw: &wgpu::Buffer,
    common_groups: [(&wgpu::BindGroup, &[u32]); 3],
    sort_groups: &[&wgpu::BindGroup],
    #[cfg(any(feature = "render_diagnostics", feature = "gpu_timing"))] timestamps: Option<
        &crate::timestamps::GaussianTimestampInterval,
    >,
) {
    encoder.clear_buffer(&scratch.sorting_global_buffer, 0, None);
    encoder.clear_buffer(&scratch.sorting_status_counter_buffer, 0, None);
    encoder.clear_buffer(draw, 0, None);
    let tiles = count.div_ceil(defs.workgroup_entries_c);
    let initial = defs.radix_initial_parity();
    let begin_pass = |encoder: &mut wgpu::CommandEncoder, group: usize, _digit: Option<u32>| {
        let descriptor = wgpu::ComputePassDescriptor {
            #[cfg(any(feature = "render_diagnostics", feature = "gpu_timing"))]
            timestamp_writes: timestamps
                .and_then(|hook| hook.writes(_digit, defs.radix_digit_places)),
            ..default()
        };
        let mut pass = encoder.begin_compute_pass(&descriptor).forget_lifetime();
        for (index, (bind_group, offsets)) in common_groups.iter().enumerate() {
            pass.set_bind_group(index as u32, *bind_group, offsets);
        }
        pass.set_bind_group(3, sort_groups[group], &[]);
        pass
    };
    {
        let mut pass = begin_pass(encoder, initial, None);
        pass.set_pipeline(pipelines[RADIX_PIPELINE_RESET]);
        pass.dispatch_workgroups(1, 1, 1);
        if compact {
            for (stage, groups) in [
                (COMPACT_COUNT, tiles),
                (COMPACT_SCAN, 1),
                (COMPACT_SCATTER, tiles),
            ] {
                pass.set_pipeline(pipelines[stage]);
                pass.dispatch_workgroups(1, groups, 1);
            }
        } else {
            pass.set_pipeline(pipelines[RADIX_PIPELINE_A]);
            pass.dispatch_workgroups(count.div_ceil(defs.workgroup_entries_a), 1, 1);
        }
        pass.set_pipeline(pipelines[RADIX_PIPELINE_B]);
        pass.dispatch_workgroups(1, defs.radix_digit_places, 1);
    }
    // A distinct INDIRECT allocation avoids writable-storage/indirect usage conflicts.
    if compact {
        let offset = (defs.radix_base * defs.radix_digit_places + 1 + defs.radix_base) as u64 * 4;
        encoder.copy_buffer_to_buffer(
            &scratch.sorting_global_buffer,
            offset,
            &scratch.dispatch_indirect_buffer,
            0,
            12,
        );
    }
    for digit in 0..defs.radix_digit_places as usize {
        let group = digit * 2 + (initial + digit) % 2;
        let mut pass = begin_pass(encoder, group, Some(digit as u32));
        for stage in [
            RADIX_PIPELINE_C_COUNT,
            RADIX_PIPELINE_C_SCAN,
            RADIX_PIPELINE_C_SCATTER,
        ] {
            pass.set_pipeline(pipelines[stage]);
            if stage == RADIX_PIPELINE_C_SCAN {
                pass.dispatch_workgroups(1, 1, 1);
            } else if compact {
                pass.dispatch_workgroups_indirect(&scratch.dispatch_indirect_buffer, 0);
            } else {
                pass.dispatch_workgroups(1, tiles, 1);
            }
        }
    }
}

#[cfg(test)]
#[path = "radix_compaction_tests.rs"]
mod compaction_tests;
