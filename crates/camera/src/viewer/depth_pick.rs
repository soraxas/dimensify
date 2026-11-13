//! Answer [`NavigationPick`] requests from the camera's rendered depth buffer.
//!
//! After the main pass, a tiny compute pass copies a window of depth around the
//! pointer into a storage buffer, which is read back asynchronously. (WebGPU only
//! copies whole depth textures to buffers, so a direct partial copy is invalid.)
//! The nearest written depth in the window wins, so sparse points, which have
//! no surface, are picked within a screen-space radius. This reflects exactly
//! what was drawn: point size, LOD and clipping. Backends without compute
//! (WebGL2) and multisampled depth answer "miss" at once, keeping the
//! focus-plane pivot.
use super::{NavigationPick, PickRequest, PickResponder, ViewerCamera};
use bevy::{
    core_pipeline::schedule::{Core3d, Core3dSystems},
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        render_resource::{
            BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, BindingResource, BindingType,
            Buffer, BufferBindingType, BufferDescriptor, BufferInitDescriptor, BufferUsages,
            ComputePassDescriptor, ComputePipeline, DownlevelFlags, MapMode,
            PipelineLayoutDescriptor, PollType, RawComputePipelineDescriptor,
            ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureSampleType, TextureUsages,
            TextureViewDimension,
        },
        renderer::{RenderAdapter, RenderContext, RenderDevice, ViewQuery},
        view::{ExtractedView, ViewDepthTexture},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const SHADER: &str = r"
@group(0) @binding(0) var depth: texture_depth_2d;
// origin.xy, size.xy in target pixels
@group(0) @binding(1) var<uniform> window: vec4<u32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= window.z || id.y >= window.w {
        return;
    }
    out[id.y * window.z + id.x] = textureLoad(depth, window.xy + id.xy, 0);
}
";

/// `Core3d` ordering for depth picking. Renderers whose visible output does not
/// normally write depth (for example sorted Gaussian splats) draw pick depth in
/// [`DepthPickSystems::Contribute`] when [`DepthPickFrame`] names their view.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DepthPickSystems {
    Contribute,
    Copy,
}

/// Render world: the main-world camera whose depth window is copied this frame.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DepthPickFrame(pub Option<Entity>);

/// Enable [`NavigationPick`] and service it from rendered depth.
pub struct DepthPickPlugin;

impl Plugin for DepthPickPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut()
            .get_resource_or_init::<NavigationPick>()
            .enabled = true;
        app.add_systems(PostUpdate, allow_depth_reads);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<DepthPickJobs>()
            .init_resource::<DepthPickFrame>()
            .add_systems(ExtractSchedule, extract_request)
            .configure_sets(
                Core3d,
                (
                    DepthPickSystems::Contribute.after(Core3dSystems::MainPass),
                    DepthPickSystems::Copy
                        .after(DepthPickSystems::Contribute)
                        .before(Core3dSystems::EarlyPostProcess),
                ),
            )
            .add_systems(Core3d, copy_depth_window.in_set(DepthPickSystems::Copy))
            .add_systems(Render, map_jobs.in_set(RenderSystems::Cleanup));
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        let world = render_app.world_mut();
        let supported = world
            .get_resource::<RenderAdapter>()
            .is_some_and(|adapter| {
                adapter
                    .get_downlevel_capabilities()
                    .flags
                    .contains(DownlevelFlags::COMPUTE_SHADERS)
            });
        if let (true, Some(device)) = (supported, world.get_resource::<RenderDevice>()) {
            let pipeline = DepthPickPipeline::new(device);
            world.insert_resource(pipeline);
        }
    }
}

/// Sampling depth in compute needs `TEXTURE_BINDING`; add it to the navigation camera only.
fn allow_depth_reads(mut cameras: Query<&mut Camera3d, With<ViewerCamera>>) {
    for mut camera in &mut cameras {
        let usages = TextureUsages::from(camera.depth_texture_usages);
        if !usages.contains(TextureUsages::TEXTURE_BINDING) {
            camera.depth_texture_usages = (usages | TextureUsages::TEXTURE_BINDING).into();
        }
    }
}

#[derive(Resource)]
struct DepthPickPipeline {
    layout: BindGroupLayout,
    pipeline: ComputePipeline,
}

impl DepthPickPipeline {
    fn new(device: &RenderDevice) -> Self {
        let entry = |binding, ty| BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let buffer = |ty| BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layout = device.create_bind_group_layout(
            "depth_pick_layout",
            &[
                entry(
                    0,
                    BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(1, buffer(BufferBindingType::Uniform)),
                entry(2, buffer(BufferBindingType::Storage { read_only: false })),
            ],
        );
        let module = device.create_and_validate_shader_module(ShaderModuleDescriptor {
            label: Some("depth_pick"),
            source: ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("depth_pick"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&RawComputePipelineDescriptor {
            label: Some("depth_pick"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: default(),
            cache: None,
        });
        Self { layout, pipeline }
    }
}

struct RecordedJob {
    request: PickRequest,
    responder: PickResponder,
    readback: Buffer,
    origin: UVec2,
    size: UVec2,
    viewport: UVec4,
    clip_from_view: Mat4,
    world_from_view: GlobalTransform,
}

#[derive(Resource, Default)]
struct DepthPickJobs {
    request: Option<(PickRequest, PickResponder)>,
    /// Last request copied; a request persists in the main world until answered.
    last_recorded: Option<u64>,
    recorded: Vec<RecordedJob>,
    /// Maps awaiting their callback; the device is polled while nonzero.
    in_flight: Arc<AtomicUsize>,
}

fn extract_request(
    pick: Extract<Option<Res<NavigationPick>>>,
    mut jobs: ResMut<DepthPickJobs>,
    mut frame: ResMut<DepthPickFrame>,
) {
    jobs.request = pick
        .as_ref()
        .filter(|pick| pick.enabled)
        .and_then(|pick| Some((pick.request()?, pick.responder())))
        .filter(|(request, _)| jobs.last_recorded != Some(request.id));
    frame.0 = jobs.request.as_ref().map(|(request, _)| request.camera);
}

fn copy_depth_window(
    view: ViewQuery<(&ExtractedView, &ViewDepthTexture, &Msaa)>,
    pipeline: Option<Res<DepthPickPipeline>>,
    mut jobs: ResMut<DepthPickJobs>,
    mut ctx: RenderContext,
) {
    let (extracted, depth, msaa) = view.into_inner();
    let Some((request, responder)) = jobs.request.clone() else {
        return;
    };
    if extracted.retained_view_entity.main_entity.id() != request.camera {
        return;
    }
    jobs.request = None;
    jobs.last_recorded = Some(request.id);
    let size = depth.texture.size();
    let supported = msaa.samples() == 1
        && depth
            .texture
            .usage()
            .contains(TextureUsages::TEXTURE_BINDING);
    let (Some(pipeline), true) = (pipeline, supported) else {
        responder.respond(request.id, None);
        return;
    };
    if request.pixel.x >= size.width || request.pixel.y >= size.height {
        responder.respond(request.id, None);
        return;
    }
    let max = UVec2::new(size.width - 1, size.height - 1);
    let origin = request.pixel.saturating_sub(UVec2::splat(request.radius));
    let window = (request.pixel + UVec2::splat(request.radius)).min(max) - origin + UVec2::ONE;
    let bytes = u64::from(window.x * window.y) * 4;
    let device = ctx.render_device().clone();
    let params = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("depth_pick_window"),
        contents: &[origin.x, origin.y, window.x, window.y]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<u8>>(),
        usage: BufferUsages::UNIFORM,
    });
    let output = device.create_buffer(&BufferDescriptor {
        label: Some("depth_pick_output"),
        size: bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("depth_pick_readback"),
        size: bytes,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(
        "depth_pick",
        &pipeline.layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(depth.view()),
            },
            BindGroupEntry {
                binding: 1,
                resource: params.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: output.as_entire_binding(),
            },
        ],
    );
    let encoder = ctx.command_encoder();
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("depth_pick"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &*bind_group, &[]);
        pass.dispatch_workgroups(window.x.div_ceil(8), window.y.div_ceil(8), 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
    jobs.recorded.push(RecordedJob {
        request,
        responder,
        readback,
        origin,
        size: window,
        viewport: extracted.viewport,
        clip_from_view: extracted.clip_from_view,
        world_from_view: extracted.world_from_view,
    });
}

/// Map after submission; the callback answers from whichever thread polls.
/// Bevy does not poll the device every frame, so poll (non-blocking) while a
/// pick is in flight; on the web the browser drives callbacks instead.
fn map_jobs(mut jobs: ResMut<DepthPickJobs>, device: Res<RenderDevice>) {
    if jobs.in_flight.load(Ordering::Acquire) > 0 {
        let _ = device.poll(PollType::Poll);
    }
    let in_flight = jobs.in_flight.clone();
    for job in jobs.recorded.drain(..) {
        in_flight.fetch_add(1, Ordering::AcqRel);
        let in_flight = in_flight.clone();
        let buffer = job.readback.clone();
        buffer.slice(..).map_async(MapMode::Read, move |result| {
            let point = result.ok().and_then(|()| {
                let data = job.readback.slice(..).get_mapped_range();
                let depths: Vec<f32> = data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| f32::from_le_bytes(*bytes))
                    .collect();
                let point = job.resolve(&depths);
                drop(data);
                job.readback.unmap();
                point
            });
            job.responder.respond(job.request.id, point);
            in_flight.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

impl RecordedJob {
    /// Choose the nearest written depth within the circular radius, then unproject.
    fn resolve(&self, depths: &[f32]) -> Option<Vec3> {
        let (depth, pixel) = nearest_depth(
            depths,
            self.origin,
            self.size,
            self.request.pixel,
            self.request.radius,
        )?;
        unproject(
            self.viewport,
            self.clip_from_view,
            &self.world_from_view,
            pixel,
            depth,
        )
    }
}

/// Largest reverse-Z depth (nearest surface) within `radius` of `center`.
fn nearest_depth(
    depths: &[f32],
    origin: UVec2,
    size: UVec2,
    center: UVec2,
    radius: u32,
) -> Option<(f32, UVec2)> {
    let limit = radius as f32;
    let mut best: Option<(f32, f32, UVec2)> = None;
    for (index, &depth) in depths.iter().enumerate().take((size.x * size.y) as usize) {
        let pixel = origin + UVec2::new(index as u32 % size.x, index as u32 / size.x);
        let distance = pixel.as_vec2().distance(center.as_vec2());
        // Reverse-Z: 0 is the cleared far plane; larger is nearer. Near-equal
        // depths (camera-facing point quads) prefer the pixel nearest the pointer.
        if depth <= 0.0 || !depth.is_finite() || distance > limit {
            continue;
        }
        let better = best.is_none_or(|(d, r, _)| {
            let tolerance = d * 1e-4;
            depth > d + tolerance || (depth >= d - tolerance && distance < r)
        });
        if better {
            best = Some((depth, distance, pixel));
        }
    }
    best.map(|(depth, _, pixel)| (depth, pixel))
}

/// Convert a target pixel and its depth into a world-space point.
fn unproject(
    viewport: UVec4,
    clip_from_view: Mat4,
    world_from_view: &GlobalTransform,
    pixel: UVec2,
    depth: f32,
) -> Option<Vec3> {
    let size = viewport.zw().as_vec2();
    if size.min_element() <= 0.0 {
        return None;
    }
    let uv = (pixel.as_vec2() + 0.5 - viewport.xy().as_vec2()) / size;
    let ndc = Vec3::new(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth);
    let view = clip_from_view.inverse().project_point3(ndc);
    let world = world_from_view.transform_point(view);
    world.is_finite().then_some(world)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unproject_inverts_perspective_reverse_z() {
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_2,
            aspect_ratio: 1.0,
            near: 0.1,
            ..default()
        });
        let camera = GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 6.0));
        // A point 4 units ahead projects to depth near / 4 at the viewport centre.
        let point = unproject(
            UVec4::new(0, 0, 100, 100),
            projection.get_clip_from_view(),
            &camera,
            UVec2::new(50, 50),
            0.1 / 4.0,
        )
        .unwrap();
        assert!(point.distance(Vec3::new(0.0, 0.0, 2.0)) < 0.1, "{point}");
    }

    #[test]
    fn nearest_depth_respects_the_circular_radius() {
        // 3x3 window at the origin; centre pixel (1, 1).
        let mut depths = [0.0; 9];
        depths[0] = 0.9; // corner: sqrt(2) away, outside radius 1
        depths[1] = 0.2; // edge: inside
        depths[4] = 0.1; // centre: inside but farther
        let hit = nearest_depth(&depths, UVec2::ZERO, UVec2::splat(3), UVec2::ONE, 1);
        assert_eq!(hit, Some((0.2, UVec2::new(1, 0))));
        assert_eq!(
            nearest_depth(&[0.0; 9], UVec2::ZERO, UVec2::splat(3), UVec2::ONE, 1),
            None
        );
    }
}
