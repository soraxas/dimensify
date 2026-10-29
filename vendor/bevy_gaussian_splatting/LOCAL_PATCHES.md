# Maintained renderer patches

This package is the canonical renderer source used by Dimensify consumers.
Upstream: `mosure/bevy_gaussian_splatting`, via the `soraxas` fork at
`b2c8e78848a25f71cb68bc62bf62148e6ad9bf0a` (version 8.0.2).
Original MIT/Apache-2.0 notices and upstream tools/fixtures are retained.

The maintained patch surface includes:

- Bevy 0.19 compatibility and optional WebGPU / CPU-sorted WebGL backends.
- Validated bundled/streamed SOG primitives and shared source ownership.
- Checked ownership transfer of expanded SOG attribute planes, with a copy
  fallback for incompatible allocation layouts or capacities. Preserve exact
  attribute bytes and reject mismatched plane lengths.
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

GPU timing remains opt-in through `GaussianGpuTimingPlugin` and tagged
`GaussianTimingView` / `GaussianTimingSource` entities. `report()` retains the
radix interval (including compaction, excluding preceding buffer clears).
`pass_reports()` provides separate bounded histories for Gaussian raster,
encoded-compositing encode/decode, and cached-projection compute passes. Each
scope has four independent in-flight slots and preserves originating
frame/view/scene/selection IDs, raw timestamp ticks and the actual device period.
Pending, unsupported, failed and empty histories are not zero-duration samples.

Raster/transfer attribution requires every active draw in the measured range to
have one matching Gaussian scene/selection and compaction policy. Ordinary
raster timing covers a complete Gaussian-only transparent phase without adding
or splitting passes; mixed transparent phases, OIT, resolution overrides and
cross-entity batches retain ordinary rendering without that sample. Encoded
compositing timestamps its existing eligible segments and separate fullscreen
transfer passes. Projection timestamps only executed tagged compute dispatches;
vertex projection is included in raster work. SH baking, buffer clears/copies,
opaque rendering, postprocessing, presentation and total-frame GPU duration are
not measured by these scopes. Multiple segments may share originating IDs;
consumers must retain all completed samples rather than deduplicate by frame.
