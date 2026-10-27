# Maintained renderer patches

This package is the canonical renderer source used by Dimensify consumers.
Upstream: `mosure/bevy_gaussian_splatting`, via the `soraxas` fork at
`b2c8e78848a25f71cb68bc62bf62148e6ad9bf0a` (version 8.0.2).
Original MIT/Apache-2.0 notices and upstream tools/fixtures are retained.

The maintained patch surface includes:

- Bevy 0.19 compatibility and optional WebGPU / CPU-sorted WebGL backends.
- Validated bundled/streamed SOG primitives and shared source ownership.
- Bounded packed GPU uploads, shared palettes, compact view-dependent baking
  and geometry/color projection invalidation.
- Render-ready acknowledgements for replacement handoffs.
- Opt-in encoded compositing, normalized rasterization and memory accounting.
- Sort compaction, CPU worker scheduling and opt-in GPU timing.

Keep GPU layout, raster, sorting, upload and readiness changes here. Cache,
LOD/admission, source resolution, camera navigation and UI belong in shared
viewer packages or their consuming adapters. The renderer must not acquire
provider, account, product or room policy.

Public diagnostic globals use the `__DIMENSIFY_` namespace. Tests and tools
must consume that namespace. Update this ledger for future generic patches;
there is one implementation and no synchronized source mirror.
