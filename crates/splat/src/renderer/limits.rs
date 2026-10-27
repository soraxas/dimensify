//! One source of truth for admission and its UI explanation. These are policy
//! and binding ceilings, not a measurement of free device memory.
use super::{SogRendererBackend, gpu, gpu_splat_limit};
use bevy::render::renderer::RenderDevice;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct SelectionLimits {
    pub device_available: bool,
    pub binding_bytes: u64,
    pub device_splats: usize,
    pub effective_splats: usize,
    pub selected_budget_bytes: u64,
    pub projection_cache: bool,
    pub webgl_texture_dimension: Option<u32>,
}

impl SelectionLimits {
    /// Derive admission ceilings from device capabilities and configured budgets.
    pub fn from_device(
        device: Option<&RenderDevice>,
        backend: SogRendererBackend,
        budget: u64,
        cameras: usize,
    ) -> Self {
        if cfg!(all(feature = "webgl", not(feature = "webgpu"))) {
            use bevy_gaussian_splatting::render::webgl::selection_limits;
            let (dimension, buffer_bytes) = device.map_or((2048, 16 * 1024 * 1024), |device| {
                let limits = device.limits();
                (limits.max_texture_dimension_2d, limits.max_buffer_size)
            });
            let (device_splats, effective_splats) =
                selection_limits(dimension, buffer_bytes, budget, cameras);
            return Self {
                device_available: device.is_some(),
                binding_bytes: buffer_bytes,
                device_splats,
                effective_splats,
                selected_budget_bytes: budget,
                projection_cache: false,
                webgl_texture_dimension: Some(dimension),
            };
        }
        let bytes = device.map_or(128 * 1024 * 1024, |device| {
            let limits = device.limits();
            limits
                .max_storage_buffer_binding_size
                .min(limits.max_buffer_size)
        });
        Self::calculate(bytes, device.is_some(), backend, budget, cameras)
    }

    fn calculate(
        bytes: u64,
        device_available: bool,
        backend: SogRendererBackend,
        budget: u64,
        cameras: usize,
    ) -> Self {
        let binding_limit = if backend == SogRendererBackend::Packed {
            ((bytes / 16).min(usize::MAX as u64) as usize / 32) * 32
        } else {
            gpu_splat_limit(bytes)
        };
        let (device_splats, effective_splats) = if backend == SogRendererBackend::Snapshot {
            (
                gpu::snapshot_limit(u64::MAX, binding_limit, bytes, cameras),
                gpu::snapshot_limit(budget, binding_limit, bytes, cameras),
            )
        } else if backend == SogRendererBackend::Packed {
            (
                binding_limit,
                super::packed::selected_limit(budget, binding_limit),
            )
        } else {
            (binding_limit, gpu::selected_limit(budget, binding_limit))
        };
        Self {
            device_available,
            binding_bytes: bytes,
            device_splats,
            effective_splats,
            selected_budget_bytes: budget,
            projection_cache: false,
            webgl_texture_dimension: None,
        }
    }

    /// Packed projection is an additional 32-byte plane in each old/new slot.
    pub fn with_projection_cache(mut self, enabled: bool) -> Self {
        if enabled {
            self.projection_cache = true;
            self.device_splats =
                ((self.binding_bytes / 32).min(usize::MAX as u64) as usize / 32) * 32;
            self.effective_splats = super::packed::selected_limit_with_projection(
                self.selected_budget_bytes,
                self.device_splats,
                true,
            );
        }
        self
    }

    /// Describe the effective admission ceilings for diagnostics.
    pub fn explanation(self) -> String {
        if let Some(dimension) = self.webgl_texture_dimension {
            return format!(
                "WebGL allocation ceiling: {} splats/scene; limited by {}.\n{} texture dimension: {}; index buffer limit: {} MiB; layout capacity: {} splats.\nConfigured GPU selection budget: {} MiB/scene for old + new attribute textures and per-view indices. This is application policy, not detected free VRAM.\nDecoded sources, CPU snapshots and worker sort scratch use additional memory; source admission can lower the effective ceiling.",
                self.effective_splats,
                self.constraint(),
                if self.device_available {
                    "Device"
                } else {
                    "Provisional (device unavailable)"
                },
                dimension,
                self.binding_bytes / (1024 * 1024),
                self.device_splats,
                self.selected_budget_bytes / (1024 * 1024),
            );
        }
        format!(
            "Renderer cap: {} splats/scene; limited by {}.\nDevice binding{}: {} MiB, up to {} splats including sort layout.\nConfigured two-generation selection: {} MiB/scene (old + new attributes, sort entries, projection and radix reservation).\nProjection: {}. CPU/source admission can lower this further; this is not free VRAM.",
            self.effective_splats,
            if self.effective_splats < self.device_splats {
                "configured selection memory"
            } else {
                "device binding / buffer size"
            },
            if self.device_available {
                ""
            } else {
                " (128 MiB fallback; device unavailable)"
            },
            self.binding_bytes / (1024 * 1024),
            self.device_splats,
            self.selected_budget_bytes / (1024 * 1024),
            if self.projection_cache {
                "compute cache (32 bytes/candidate/slot)"
            } else {
                "vertex (no cache)"
            },
        )
    }

    /// Name the constraint actually used by renderer admission, not free memory.
    pub fn constraint(self) -> &'static str {
        if self.effective_splats < self.device_splats {
            "GPU selection budget"
        } else if self.webgl_texture_dimension.is_some() {
            "Texture / index limits"
        } else {
            "Device binding / buffer limits"
        }
    }

    /// Explain the tightest known constraint on the user's selection request.
    pub fn request_constraint(
        self,
        requested: usize,
        effective: usize,
        source_limited: bool,
    ) -> &'static str {
        if source_limited || effective < requested.min(self.effective_splats) {
            "Source cache budget"
        } else {
            self.constraint()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
    #[test]
    fn webgl_capacity_uses_texture_index_and_two_generation_memory_limits() {
        // With no device yet, the 2048-square texture holds 4 texels per splat.
        let device = SelectionLimits::from_device(None, SogRendererBackend::Snapshot, 512 << 20, 1);
        assert_eq!(device.device_splats, 1_048_576);
        assert_eq!(device.effective_splats, 1_048_576);
        // Two generations, each with 512 * 64 attribute bytes and 512 * 4 indices.
        let policy = SelectionLimits::from_device(None, SogRendererBackend::Snapshot, 69_632, 1);
        assert_eq!(policy.effective_splats, 512);
        // A second view needs another index stream; the next texture row cannot fit.
        let stereo = SelectionLimits::from_device(None, SogRendererBackend::Snapshot, 69_632, 2);
        assert_eq!(stereo.effective_splats, 256);
        assert_eq!(
            SelectionLimits::from_device(None, SogRendererBackend::Snapshot, 0, 1).effective_splats,
            0
        );
    }

    #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
    #[test]
    fn webgl_names_source_pressure_when_it_is_tighter_than_renderer_capacity() {
        let limits = SelectionLimits::from_device(None, SogRendererBackend::Snapshot, 512 << 20, 1);
        assert_eq!(
            limits.request_constraint(2_000_000, 750_000, false),
            "Source cache budget"
        );
        assert_eq!(
            limits.request_constraint(2_000_000, 1_048_576, false),
            "Texture / index limits"
        );
        assert_eq!(
            limits.request_constraint(1_000_000, 1_000_000, true),
            "Source cache budget"
        );
    }

    #[test]
    fn sh3_screenshot_cap_is_policy_not_free_vram() {
        if bevy_gaussian_splatting::material::spherical_harmonics::SH_DEGREE != 3 {
            return;
        }
        for backend in [
            SogRendererBackend::Snapshot,
            SogRendererBackend::GpuResident,
        ] {
            let generous_device =
                SelectionLimits::calculate(1024 * 1024 * 1024, true, backend, 512 * 1024 * 1024, 1);
            assert_eq!(generous_device.effective_splats, 1_044_448);
            assert!(
                generous_device
                    .explanation()
                    .contains("limited by configured selection memory")
            );
            let small_device =
                SelectionLimits::calculate(128 * 1024 * 1024, true, backend, 512 * 1024 * 1024, 1);
            assert_eq!(small_device.effective_splats, 699_040);
            assert!(
                small_device
                    .explanation()
                    .contains("limited by device binding")
            );
        }
    }
}
