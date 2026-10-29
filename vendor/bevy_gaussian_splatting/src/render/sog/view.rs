//! Per-view SH input ownership. Immutable sources remain pinned by the draw
//! generation; dropping that generation releases its references.
use super::{
    GpuSogSource,
    diagnostics::BakeDiagnostics,
    work::{BakeRange, CompactBaker, CompactWork},
};
use crate::render::compact::CompactCloud;
use bevy::{
    prelude::*,
    render::{render_resource::Buffer, renderer::WgpuWrapper},
};
use std::sync::Arc;

mod reuse;

pub struct SourceRange {
    pub source: Arc<WgpuWrapper<GpuSogSource>>,
    pub range: BakeRange,
}

pub struct Sources {
    pub(super) work: Arc<WgpuWrapper<CompactWork>>,
    pub(super) baker: Arc<WgpuWrapper<CompactBaker>>,
    pub(super) ranges: Vec<SourceRange>,
    color_refresh_ratio: f32,
    reuse_from: std::sync::Weak<WgpuWrapper<CompactWork>>,
}

impl CompactCloud {
    /// Describe a dense selected generation. Geometry and view-dependent colour
    /// are baked in the render schedule before sorting, never from main update.
    /// The caller reserves work/source/sort/transition memory before construction.
    pub fn from_sources(
        work: Arc<WgpuWrapper<CompactWork>>,
        baker: Arc<WgpuWrapper<CompactBaker>>,
        ranges: Vec<SourceRange>,
        indirect: Buffer,
        view: Entity,
    ) -> Result<Self, &'static str> {
        let mut count = 0u32;
        for entry in &ranges {
            entry
                .range
                .validate(entry.source.count(), work.capacity())?;
            if entry.range.count == 0 || entry.range.destination_start != count {
                return Err("compact source ranges must densely cover the selection in order");
            }
            count = count
                .checked_add(entry.range.count)
                .ok_or("compact selection overflow")?;
        }
        let mut cloud = Self::new(
            work.storage_buffers().map(Clone::clone),
            indirect,
            count,
            view,
        )?;
        cloud.sources = Some(Arc::new(Sources {
            work,
            baker,
            ranges,
            color_refresh_ratio: 0.0,
            reuse_from: Default::default(),
        }));
        cloud.ready = false;
        Ok(cloud)
    }

    /// Configure distance-scaled SH colour refresh before publishing a cloud.
    /// Zero (the generic default) refreshes exactly; 10 degrees matches the
    /// reference viewer's translation heuristic. Sorting/LOD are unaffected.
    pub fn set_color_refresh_angle(&mut self, degrees: f32) -> Result<(), &'static str> {
        if !degrees.is_finite() || !(0.0..=45.0).contains(&degrees) {
            return Err("colour refresh angle must be finite and between 0 and 45 degrees");
        }
        let sources = self
            .sources
            .as_mut()
            .and_then(Arc::get_mut)
            .ok_or("configure colour refresh before sharing source generation")?;
        sources.color_refresh_ratio = degrees.to_radians().tan();
        Ok(())
    }

    /// Identify another admitted slot as a potential donor. The render cache
    /// uses it only after a successful bake; this weak link retains no GPU data.
    pub fn reuse_work_from(
        &mut self,
        work: &Arc<WgpuWrapper<CompactWork>>,
    ) -> Result<(), &'static str> {
        let sources = self
            .sources
            .as_mut()
            .and_then(Arc::get_mut)
            .ok_or("configure work reuse before sharing source generation")?;
        if Arc::ptr_eq(&sources.work, work) {
            return Err("donor must be a different working slot");
        }
        sources.reuse_from = Arc::downgrade(work);
        Ok(())
    }
}

/// Source-space camera input, shared by the change key and the actual baker.
fn local_view(
    view: &bevy::render::view::ExtractedView,
    transform: &GlobalTransform,
) -> Result<super::work::LocalView, &'static str> {
    let inverse = transform.affine().inverse();
    if !inverse.is_finite() {
        return Err("non-invertible compact cloud transform");
    }
    if view.clip_from_view.w_axis.w == 1.0 {
        super::work::LocalView::orthographic(
            inverse
                .transform_vector3(*view.world_from_view.forward())
                .to_array(),
        )
    } else {
        super::work::LocalView::perspective(
            inverse
                .transform_point3(view.world_from_view.translation())
                .to_array(),
        )
    }
}

// Weak identities never pin GPU sources if the last camera is removed and the
// per-view schedule stops running before its next cleanup pass.
#[derive(Default)]
pub(crate) struct BakeCache(std::collections::HashMap<usize, BakeState>);

impl BakeCache {
    pub(crate) fn bake<'a>(
        &mut self,
        cloud: &mut CompactCloud,
        local: super::work::LocalView,
        device: &wgpu::Device,
        encoder: impl FnOnce() -> &'a mut wgpu::CommandEncoder,
        diagnostics: &BakeDiagnostics,
    ) {
        self.0.retain(|_, state| state.work.strong_count() > 0);
        let Some(sources) = cloud.sources.clone() else {
            return;
        };
        let key = Arc::as_ptr(&sources.work) as usize;
        let mut state = self.0.remove(&key).unwrap_or_default();
        state.bake_from(
            cloud,
            local,
            device,
            encoder,
            diagnostics,
            self.0.get(&(sources.reuse_from.as_ptr() as usize)),
        );
        self.0.insert(key, state);
    }
}

/// Per-cloud cache used by the production view system and offscreen probes.
/// Weak identity must not extend a generation's source residency.
#[derive(Default)]
pub(crate) struct BakeState {
    previous: Option<(std::sync::Weak<Sources>, super::work::LocalView)>,
    work: std::sync::Weak<WgpuWrapper<CompactWork>>,
    contents: Vec<reuse::CachedRange>,
}

impl BakeState {
    #[cfg(test)]
    pub(crate) fn bake<'a>(
        &mut self,
        cloud: &mut CompactCloud,
        local: super::work::LocalView,
        device: &wgpu::Device,
        encoder: impl FnOnce() -> &'a mut wgpu::CommandEncoder,
        diagnostics: &BakeDiagnostics,
    ) {
        self.bake_from(cloud, local, device, encoder, diagnostics, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn bake_from<'a>(
        &mut self,
        cloud: &mut CompactCloud,
        local: super::work::LocalView,
        device: &wgpu::Device,
        encoder: impl FnOnce() -> &'a mut wgpu::CommandEncoder,
        diagnostics: &BakeDiagnostics,
        donor: Option<&BakeState>,
    ) {
        let Some(sources) = cloud.sources.clone() else {
            *self = Self::default();
            return;
        };
        let same_sources = self
            .previous
            .as_ref()
            .is_some_and(|(old, _)| old.as_ptr() == Arc::as_ptr(&sources));
        if self
            .previous
            .as_ref()
            .is_some_and(|(_, old)| same_sources && *old == local)
        {
            cloud.ready = true;
            diagnostics.record(|r| r.cache_hits = r.cache_hits.saturating_add(1));
            return;
        }
        diagnostics.record(|r| {
            let updates = if same_sources {
                &mut r.color_updates
            } else {
                &mut r.full_updates
            };
            *updates = updates.saturating_add(1);
        });
        let max_count = device
            .limits()
            .max_compute_workgroups_per_dimension
            .saturating_mul(64);
        if max_count == 0 {
            cloud.ready = false;
            *self = Self::default();
            diagnostics.record(|r| r.failed_updates = r.failed_updates.saturating_add(1));
            return;
        }
        let plan = reuse::Plan::new(&sources, local, self, donor);
        if !plan.has_work() {
            cloud.ready = true;
            self.previous = Some((Arc::downgrade(&sources), local));
            self.work = Arc::downgrade(&sources.work);
            self.contents = plan.contents;
            return;
        }
        // RenderContext allocates/queues an encoder when requested, even if no
        // commands follow. Preserve the original lazy acquisition on cache hits.
        let encoder = encoder();
        plan.copy(&sources.work, encoder, diagnostics);
        let success = [false, true].into_iter().all(|color_only| {
            let dispatches = plan.dispatches(&sources, color_only, max_count);
            sources
                .baker
                .encode_ranges_observed(
                    device,
                    encoder,
                    &sources.work,
                    dispatches,
                    local,
                    color_only,
                    diagnostics,
                )
                .is_ok()
        });
        cloud.ready = success;
        if success {
            self.previous = Some((Arc::downgrade(&sources), local));
            self.work = Arc::downgrade(&sources.work);
            self.contents = plan.contents;
        } else {
            *self = Self::default();
            diagnostics.record(|r| r.failed_updates = r.failed_updates.saturating_add(1));
        }
    }
}

/// Runs on the render encoder before radix/draw for the owning view. Reusing
/// unchanged SH does not suppress sorting or projection/FOV updates.
#[cfg(feature = "sort_radix")]
pub(crate) fn bake_view(
    mut context: bevy::render::renderer::RenderContext,
    device: Res<bevy::render::renderer::RenderDevice>,
    view: bevy::render::renderer::ViewQuery<&bevy::render::view::ExtractedView>,
    mut clouds: Query<(Entity, &mut CompactCloud, &GlobalTransform)>,
    mut cache: Local<BakeCache>,
    diagnostics: Option<Res<BakeDiagnostics>>,
) {
    let view_entity = view.entity();
    let view = view.into_inner();
    let disabled = BakeDiagnostics::default();
    let diagnostics = diagnostics.as_deref().unwrap_or(&disabled);
    for (_, mut cloud, transform) in &mut clouds {
        if cloud.view() != view_entity {
            continue;
        }
        if cloud.sources.is_none() {
            continue;
        }
        let Ok(local) = local_view(view, transform) else {
            cloud.ready = false;
            continue;
        };
        cache.bake(
            &mut cloud,
            local,
            device.wgpu_device(),
            || context.command_encoder(),
            diagnostics,
        );
    }
}
