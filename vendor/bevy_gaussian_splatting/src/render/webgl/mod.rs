//! WebGL2 3D Gaussian fallback: worker depth sorting and persistent attributes.
//! No storage/compute/indirect resources are installed by this plugin.

mod ordering;
mod pipeline;
use crate::{
    CloudSettings, Gaussian3d, PlanarGaussian3d, PlanarGaussian3dHandle,
    gaussian::settings::GaussianColorSpace,
};
use bevy::{
    prelude::*,
    render::{Extract, ExtractSchedule, RenderApp},
};
use bytemuck::{Pod, Zeroable};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

/// Explicit per-cloud GPU allocation policy for two selected generations.
/// This is a configurable application budget, not detected free GPU memory.
#[derive(Resource)]
pub struct SelectedGpuBudget(pub u64);

impl Default for SelectedGpuBudget {
    fn default() -> Self {
        Self(512 * 1024 * 1024)
    }
}

/// Return texture/index capacity and the capacity admitted by the GPU budget.
/// Attribute rows contain at most 512 splats; each view owns a u32 index stream.
pub fn selection_limits(
    dimension: u32,
    buffer_bytes: u64,
    budget: u64,
    cameras: usize,
) -> (usize, usize) {
    let texture = u64::from(dimension.min(2048) / 4) * u64::from(dimension);
    let device = texture.min(buffer_bytes / 4).min(u64::from(u32::MAX)) as usize;
    let mut low = 0;
    let mut high = device;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if selected_gpu_bytes(mid, cameras).saturating_mul(2) <= budget {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    (device, low)
}

/// Packed four-texel record texture extent, or None when device bounds reject it.
pub fn attribute_extent(count: usize, max_dimension: u32) -> Option<(u32, u32)> {
    if count == 0 || max_dimension < 4 {
        return None;
    }
    let row = count
        .min(512)
        .next_power_of_two()
        .min(max_dimension as usize / 4);
    let height = count.div_ceil(row);
    (height <= max_dimension as usize).then_some(((row * 4) as u32, height as u32))
}

/// GPU attribute payload plus separate view index streams, including texture padding.
pub fn selected_gpu_bytes(count: usize, cameras: usize) -> u64 {
    if count == 0 {
        return 0;
    }
    let texture = attribute_extent(count, u32::MAX)
        .map_or(u64::MAX, |(w, h)| u64::from(w) * u64::from(h) * 16);
    texture.saturating_add(
        (count as u64)
            .saturating_mul(4)
            .saturating_mul(cameras.max(1) as u64),
    )
}

/// World-space attributes consumed by the WebGL vertex stage (64 bytes).
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
#[repr(C)]
pub struct Instance {
    pub position_opacity: [f32; 4],
    pub covariance_a: [f32; 4],
    pub covariance_b: [f32; 4],
    pub color: [f32; 4],
}

/// Convert bounded, evenly sampled rows to world covariance and linear SH0 colour.
/// Invisible padding and non-finite/degenerate input rows never reach the GPU.
pub fn instances(
    cloud: &PlanarGaussian3d,
    transform: &GlobalTransform,
    settings: &CloudSettings,
    limit: usize,
) -> Vec<Instance> {
    let rows = cloud
        .position_visibility
        .len()
        .min(cloud.rotation.len())
        .min(cloud.scale_opacity.len())
        .min(cloud.spherical_harmonic.len());
    let count = rows.min(limit);
    let mut result = Vec::with_capacity(count);
    let basis = Mat3::from_mat4(transform.to_matrix());
    for sample in 0..count {
        let i = (sample as u64 * rows as u64 / count as u64) as usize;
        let p = cloud.position_visibility[i];
        let s = cloud.scale_opacity[i];
        let r = cloud.rotation[i].rotation;
        let position = transform.transform_point(Vec3::from(p.position));
        let opacity = s.opacity * settings.global_opacity;
        let scale = Vec3::from(s.scale) * settings.global_scale;
        let q = Quat::from_xyzw(r[1], r[2], r[3], r[0]);
        if (settings.draw_mode == crate::gaussian::settings::DrawMode::Selected
            && p.visibility < 0.5)
            || !p.visibility.is_finite()
            || !position.is_finite()
            || !scale.is_finite()
            || scale.min_element() <= 0.0
            || !opacity.is_finite()
            || opacity <= 0.0
            || !q.is_finite()
            || q.length_squared() < 1e-12
        {
            continue;
        }
        let m = basis * Mat3::from_quat(q.normalize()) * Mat3::from_diagonal(scale);
        let cov = m * m.transpose();
        if !cov.is_finite() {
            continue;
        }
        let sh = &cloud.spherical_harmonic[i].coefficients;
        if !sh[..3].iter().all(|v| v.is_finite()) {
            continue;
        }
        let rgb = [sh[0], sh[1], sh[2]].map(|v| (0.5 + 0.282_094_8 * v).max(0.0));
        if !rgb.iter().all(|v| v.is_finite()) {
            continue;
        }
        let rgb = rgb.map(|v| {
            if settings.color_space == GaussianColorSpace::LinRec709Display {
                v
            } else if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        if !rgb.iter().all(|v| v.is_finite()) {
            continue;
        }
        result.push(Instance {
            position_opacity: [position.x, position.y, position.z, opacity.min(0.999)],
            covariance_a: [cov.x_axis.x, cov.x_axis.y, cov.x_axis.z, cov.y_axis.y],
            covariance_b: [cov.y_axis.z, cov.z_axis.z, 0.0, 0.0],
            color: [rgb[0], rgb[1], rgb[2], 1.0],
        });
    }
    result
}

/// Sort far to near along the camera's negative Z axis, including camera rotation.
pub fn sort_instances(data: &mut [Instance], view_from_world: Mat4) {
    let depth = |s: &Instance| {
        view_from_world
            .transform_point3(Vec3::from_slice(&s.position_opacity))
            .z
    };
    data.sort_unstable_by(|a, b| depth(a).total_cmp(&depth(b)));
}

/// Prepared main-world entity IDs; populated only after buffer and pipeline readiness.
#[derive(Resource, Clone, Default)]
pub struct ReadyClouds(pub Arc<Mutex<HashSet<Entity>>>);

impl ReadyClouds {
    /// Publish only sources with a usable pipeline and first order in every view.
    pub fn publish_views(&self, views: &[HashSet<Entity>]) {
        let mut ready = views.first().cloned().unwrap_or_default();
        for view in views.iter().skip(1) {
            ready.retain(|entity| view.contains(entity));
        }
        *self.0.lock().unwrap() = ready;
    }
}

#[derive(Component, Clone)]
struct CpuCloud {
    data: Arc<Vec<Instance>>,
    limit: usize,
    normalized: bool,
    additive: bool,
    center: Vec3,
}

/// Installs the expanded 3D fallback without the storage-based Gaussian plugins.
pub struct WebGlPlugin;
impl Plugin for WebGlPlugin {
    fn build(&self, app: &mut App) {
        let ready = ReadyClouds::default();
        app.insert_resource(ready.clone())
            .init_resource::<SelectedGpuBudget>()
            .init_asset::<PlanarGaussian3d>()
            .init_asset::<crate::PlanarGaussian4d>()
            // The shared snapshot adapter retains the CPU sort asset API only.
            .init_asset::<crate::sort::SortedEntries>()
            .add_plugins((
                crate::io::IoPlugin,
                crate::camera::GaussianCameraPlugin,
                crate::gaussian::settings::SettingsPlugin,
                crate::gaussian::cloud::CloudPlugin::<Gaussian3d>::default(),
            ))
            .add_systems(
                PostUpdate,
                prepare_clouds.after(bevy::transform::TransformSystems::Propagate),
            );
        pipeline::install(app, ready);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Startup, ordering::install);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(ExtractSchedule, extract_clouds);
        }
    }
}

fn prepare_clouds(
    mut commands: Commands,
    budget: Res<SelectedGpuBudget>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
    cameras: Query<(), With<crate::GaussianCamera>>,
    clouds: Res<Assets<PlanarGaussian3d>>,
    mut events: MessageReader<AssetEvent<PlanarGaussian3d>>,
    retired: Query<Entity, (With<CpuCloud>, Without<PlanarGaussian3dHandle>)>,
    query: Query<(
        Entity,
        Ref<PlanarGaussian3dHandle>,
        Ref<GlobalTransform>,
        Ref<CloudSettings>,
        Option<&CpuCloud>,
    )>,
) {
    let (dimension, buffer_bytes) = device.as_ref().map_or((2048, 16 << 20), |device| {
        let limits = device.limits();
        (limits.max_texture_dimension_2d, limits.max_buffer_size)
    });
    let (_, limit) = selection_limits(dimension, buffer_bytes, budget.0, cameras.iter().count());
    for entity in &retired {
        commands.entity(entity).remove::<CpuCloud>();
    }
    let changed: HashSet<_> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    for (entity, handle, transform, settings, previous) in &query {
        if previous.is_some()
            && previous.is_some_and(|previous| previous.limit == limit)
            && !handle.is_changed()
            && !transform.is_changed()
            && !settings.is_changed()
            && !changed.contains(&handle.0.id())
        {
            continue;
        }
        let Some(cloud) = clouds.get(&handle.0) else {
            commands.entity(entity).remove::<CpuCloud>();
            continue;
        };
        let data = instances(cloud, &transform, &settings, limit);
        commands.entity(entity).insert(CpuCloud {
            data: Arc::new(data),
            limit,
            normalized: settings.raster_profile == crate::GaussianRasterProfile::NormalizedGaussian,
            additive: settings.additive,
            center: transform.translation(),
        });
    }
}

#[derive(Resource, Default)]
struct ExtractedClouds(Vec<(Entity, CpuCloud, bool)>);

fn extract_clouds(
    mut clouds: ResMut<ExtractedClouds>,
    query: Extract<Query<(Entity, &CpuCloud, &InheritedVisibility), With<PlanarGaussian3dHandle>>>,
) {
    clouds.0.clear();
    // Include hidden pending snapshots in preparation, never in visible draws.
    clouds
        .0
        .extend(query.iter().map(|(e, c, v)| (e, c.clone(), v.get())));
}
