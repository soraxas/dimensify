//! Explicit renderer configuration; environment and query parsing belongs to consumers.
use crate::{renderer::SogRendererBackend, streaming::SogVisibilityPolicy};
use bevy::prelude::*;
use std::collections::HashMap;
/// Startup-only experimental projection cache; independent of source storage.
#[derive(Resource, Clone, Copy, Default)]
pub struct SogProjectionCache(pub bool);

/// Optional renderer configuration for a streamed scene. Set backend and
/// projection storage before its first update; changing either requires a
/// replacement stream entity. Compaction can change live on sorted GPU paths.
/// Streams without this component retain the application-wide defaults.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SogStreamRenderSettings {
    pub(crate) backend: SogRendererBackend,
    pub(crate) projection_cache: bool,
    pub compaction: bool,
}
impl SogStreamRenderSettings {
    /// Choose storage for a new stream. Replace the stream entity to change it.
    pub const fn new(backend: SogRendererBackend) -> Self {
        Self {
            backend,
            projection_cache: false,
            compaction: false,
        }
    }
    /// Configure projection storage before attaching to a new stream entity.
    pub const fn with_projection_cache(mut self, enabled: bool) -> Self {
        self.projection_cache = enabled;
        self
    }
    pub const fn backend(&self) -> SogRendererBackend {
        self.backend
    }
    pub const fn projection_cache(&self) -> bool {
        self.projection_cache
    }
}

/// Startup-only page residency experiment. Whole-file fallback remains default.
#[derive(Resource, Clone, Copy, Default)]
pub struct SogPagedSources(pub bool);

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SogRuntimeSettings {
    pub backend: SogRendererBackend,
    pub visibility: SogVisibilityPolicy,
    pub compaction: bool,
}

impl SogRuntimeSettings {
    /// Parse explicit renderer settings and report unsupported or invalid choices.
    pub fn parse(
        renderer: Option<&str>,
        visibility: Option<&str>,
        compaction: Option<&str>,
    ) -> (Self, Vec<String>) {
        let mut settings = Self::default();
        let mut warnings = Vec::new();
        for (name, value, valid) in [
            ("sog_renderer", renderer, &["packed", "snapshot", "gpu"][..]),
            ("sog_visibility", visibility, &["all", "frustum"][..]),
            ("sog_compaction", compaction, &["off", "on"][..]),
        ] {
            if let Some(value) = value {
                if !valid.contains(&value) {
                    warnings.push(format!("invalid {name}={value:?}; using {}", valid[0]));
                    continue;
                }
                match name {
                    "sog_renderer" if value == "snapshot" => {
                        settings.backend = SogRendererBackend::Snapshot
                    }
                    "sog_renderer" if value == "gpu" => {
                        settings.backend = SogRendererBackend::GpuResident
                    }
                    "sog_renderer" if value == "packed" => {
                        settings.backend = SogRendererBackend::Packed
                    }
                    "sog_visibility" if value == "frustum" => {
                        settings.visibility = SogVisibilityPolicy::Frustum
                    }
                    "sog_compaction" => settings.compaction = value == "on",
                    _ => {}
                }
            }
        }
        #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
        {
            if renderer.is_some_and(|value| value != "snapshot") {
                warnings.push("WebGL uses CPU snapshots; GPU SOG backends are unavailable".into());
            }
            settings.backend = SogRendererBackend::Snapshot;
            settings.compaction = false;
        }
        (settings, warnings)
    }
}

/// Logical last accepted selection for each live scene, including pending work.
#[derive(Resource, Default)]
pub struct SogSelectionStatistics {
    pub counts: HashMap<Entity, usize>,
    pub submitted_generations: u64,
    pub ready_generations: u64,
}

/// Stable diagnostic name for the selected source-residency backend.
pub fn backend_name(backend: SogRendererBackend) -> &'static str {
    match backend {
        SogRendererBackend::Snapshot => "snapshot",
        SogRendererBackend::GpuResident => "gpu",
        SogRendererBackend::Packed => "packed",
    }
}

/// Opt-in application-wide transparency policy. Install `apply_transparency`
/// after scene/stream updates to configure current and pending clouds together.
/// Applications needing per-view policies should set CloudSettings directly.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SplatTransparency(pub bevy_gaussian_splatting::GaussianTransparency);

pub fn apply_transparency(
    mode: Res<SplatTransparency>,
    mut clouds: Query<&mut bevy_gaussian_splatting::CloudSettings>,
) {
    for mut settings in &mut clouds {
        if settings.transparency != mode.0 {
            settings.transparency = mode.0;
        }
    }
}

#[cfg(test)]
mod transparency_tests {
    use super::*;
    use bevy_gaussian_splatting::{CloudSettings, GaussianTransparency};

    #[test]
    fn policy_updates_existing_and_new_pending_clouds_without_changing_appearance() {
        let mut app = App::new();
        app.insert_resource(SplatTransparency(GaussianTransparency::Stochastic))
            .add_systems(Update, apply_transparency);
        let old = app
            .world_mut()
            .spawn(CloudSettings {
                global_opacity: 0.4,
                stochastic_seed: 123,
                ..default()
            })
            .id();
        app.update();
        let pending = app.world_mut().spawn(CloudSettings::default()).id();
        app.update();
        for entity in [old, pending] {
            assert!(
                app.world()
                    .get::<CloudSettings>(entity)
                    .unwrap()
                    .stochastic()
            );
        }
        app.world_mut().resource_mut::<SplatTransparency>().0 = GaussianTransparency::Sorted;
        app.update();
        for entity in [old, pending] {
            assert!(
                !app.world()
                    .get::<CloudSettings>(entity)
                    .unwrap()
                    .stochastic()
            );
        }
        let preserved = app.world().get::<CloudSettings>(old).unwrap();
        assert_eq!(preserved.global_opacity, 0.4);
        assert_eq!(preserved.stochastic_seed, 123);
    }
}
