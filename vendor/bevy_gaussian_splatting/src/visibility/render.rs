//! Render-world side of visibility fields: GPU buffers per cloud and the shared fallbacks bound
//! when a cloud has no field.
//!
//! The radix sort's key stage evaluates the field once per Gaussian per sort (`candidate_key`),
//! writes `pack2x16float(vis, keep)` into the cloud's state buffer and drops culled Gaussians from
//! the sort (and, with visibility compaction, from the draw). The vertex shader reads the state.

use std::sync::Arc;

use bevy::{
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_resource::{Buffer, BufferInitDescriptor, BufferUsages},
        renderer::RenderDevice,
    },
};

use super::{VisibilityCurve, VisibilityDisplay, VisibilityField, VisibilitySettings};

/// Attach to a Gaussian cloud entity (same Gaussian order as the cloud asset).
#[derive(Component, Clone)]
pub struct GaussianVisibility {
    pub field: Arc<VisibilityField>,
    pub settings: VisibilitySettings,
}

impl GaussianVisibility {
    pub fn new(field: VisibilityField) -> Self {
        Self { field: Arc::new(field), settings: VisibilitySettings::default() }
    }
}

/// Render-world copy of the main-world component.
#[derive(Component, Clone)]
pub struct ExtractedVisibility {
    /// Cloud length (>= field count; loaders may pad clouds, padding is never visible).
    pub count: usize,
    pub field: Arc<VisibilityField>,
    pub settings: VisibilitySettings,
}

/// GPU buffers for one cloud's field.
#[derive(Component)]
pub struct GpuVisibilityField {
    source: usize,
    pub count: usize,
    pub field: Buffer,
    pub state: Buffer,
}

impl GpuVisibilityField {
    /// Identity of the field these buffers were built from (non-zero).
    pub fn source(&self) -> usize {
        self.source
    }
}

/// Bound in place of a field for clouds without one (separate buffers so a pass never binds one
/// buffer as both read-only and writable storage).
#[derive(Resource)]
pub struct VisibilityFallback {
    pub field: Buffer,
    pub state: Buffer,
}

impl FromWorld for VisibilityFallback {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let make = |label| {
            device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some(label),
                contents: &[0u8; 64],
                usage: BufferUsages::STORAGE,
            })
        };
        Self { field: make("visibility fallback field"), state: make("visibility fallback state") }
    }
}

/// `(field, state)` buffers to bind for a cloud.
pub fn visibility_buffers<'a>(
    gpu: Option<&'a GpuVisibilityField>,
    fallback: &'a VisibilityFallback,
) -> (&'a Buffer, &'a Buffer) {
    gpu.map(|g| (&g.field, &g.state)).unwrap_or((&fallback.field, &fallback.state))
}

/// Uniform values for a cloud: `(mode, curve, lo, hi, gamma, rho, gain, overlay)`.
pub fn uniform_values(v: Option<&GaussianVisibility>, count: usize) -> (u32, u32, [f32; 7]) {
    let Some(v) = v.filter(|v| v.field.count <= count) else {
        return (0, 0, [0.3, 0.7, 1.0, 0.3, 1.0, 0.0, 0.0]);
    };
    let s = &v.settings;
    let mode = match s.display {
        VisibilityDisplay::Off => 0,
        VisibilityDisplay::Faded => 1,
        VisibilityDisplay::Removed => 2,
        VisibilityDisplay::Uncertainty => 3,
        VisibilityDisplay::Overlay => 4,
    };
    let curve = match s.curve {
        VisibilityCurve::Hide => 0,
        VisibilityCurve::Smooth => 1,
        VisibilityCurve::Linear => 2,
    };
    (mode, curve, [s.lo, s.hi, s.gamma, s.rho.unwrap_or(v.field.rho), s.gain, s.overlay, s.fit])
}

fn prepare_visibility_buffers(
    mut commands: Commands,
    device: Res<RenderDevice>,
    clouds: Query<(Entity, Option<&ExtractedVisibility>, Option<&GpuVisibilityField>)>,
) {
    for (entity, extracted, gpu) in &clouds {
        match (extracted, gpu) {
            (Some(e), gpu) => {
                let source = Arc::as_ptr(&e.field) as usize;
                if gpu.is_some_and(|g| g.source == source && g.count == e.count) {
                    continue;
                }
                // Padding Gaussians get zero evidence (never observed).
                let mut packed = Vec::with_capacity(e.count.max(1) * super::field::PACKED_WORDS);
                packed.extend_from_slice(&e.field.packed);
                packed.resize(e.count.max(1) * super::field::PACKED_WORDS, 0);
                let field = device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("visibility field"),
                    contents: bytemuck::cast_slice(&packed),
                    usage: BufferUsages::STORAGE,
                });
                let state = device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("visibility state"),
                    // vis = keep = 1 until the first sort writes real values
                    contents: bytemuck::cast_slice(&vec![0x3c00_3c00u32; e.count.max(1)]),
                    usage: BufferUsages::STORAGE,
                });
                commands.entity(entity).insert(GpuVisibilityField { source, count: e.count, field, state });
            }
            (None, Some(_)) => {
                commands.entity(entity).remove::<GpuVisibilityField>();
            }
            (None, None) => {}
        }
    }
}

pub struct VisibilityRenderPlugin;

impl Plugin for VisibilityRenderPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(Render, prepare_visibility_buffers.in_set(RenderSystems::PrepareResources));
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<VisibilityFallback>();
        }
    }
}
