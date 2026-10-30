# Comparing splat renderers

The local reference audit covers `3dgs-renderer-bench` and four implementations:
Spark, PlayCanvas Engine, SuperSplat and Gaussian-Splat-Lite. It informed the
measurement boundaries; no cross-renderer performance result is claimed.

| Reference | Behavior that matters | What to measure in Dimensify |
| --- | --- | --- |
| Spark | Conditional sorting, GPU readback before worker work, separate fetch/decode/upload queues and LRU residency | Static versus moving views; pending upload/decode and readback costs, not worker time alone |
| PlayCanvas Engine | Camera-dependent sorting, projection/compaction/raster work and throttled LOD jobs | Completed frames, sorting frequency and LOD convergence under budgets |
| SuperSplat | Cached projection, survivor compaction, editable/source allocations and alternative sorting modes | Matching rendering modes; allocated capacity versus survivors; source and scratch memory separately |
| Gaussian-Splat-Lite | WebGPU and WebGL paths, dirty-state caches and pinned in-flight streaming slots | Backend identity, cancellation, transient ownership, eviction and revisit behavior |

The reference lab waits for completion with one frame in flight, keeps RAF waiting
outside its interval, warms the pipeline and tests static/moving main and near-side
views. Our native runner adopts those boundaries. Its completed-frame wall time
includes CPU scheduling, submission and GPU waiting. A timestamp spanning passes
differs from the sum of pass times when GPU work overlaps.

The reference adapters time different portions of loading: some stop at asset
initialization, others include GPU resources or editable instances. Their numbers
cannot directly answer which viewer reaches its first completed frame sooner.
Our production browser replay therefore distinguishes loader readiness, first
encoded draw and refinement settling. It does not claim first visible pixel or
completed-frame timing from RAF.

## Conditions for a useful comparison

- Fix the dataset and child-resource hashes, physical resolution/DPR, camera
  poses, SH degree, sorting mode/precision and rendering quality settings.
- Record GPU, driver, browser, compiler/profile and instrumentation mode. Run
  one benchmark at a time; repeat fresh browser processes and rotate engine order.
- Keep static and moving views separate. Motion can invalidate caches and trigger
  sorting that a static view skips. Record poses and sort age/frequency when
  comparing asynchronous sort pipelines.
- Inspect captures. Matching configuration alone does not prove image parity.
  Allocated splat capacity is not necessarily the count surviving projection.
- Report source-cache, CPU backing, GPU allocation and transient/pending ownership
  separately. A cache counter is not process RSS or total VRAM.

!!! note "Check repeatability before interpreting a small gain"
    For mixed results, alternate two groups using the exact same executable,
    fixture and camera to measure variation without a code change. Keep the
    original unfavorable runs. Isolate decode stages separately from startup
    and rendering; warm resident-input loops do not measure cold loading.
    App-update wall time includes scheduling, submission and internal waits,
    and is not pure CPU execution. Validate profiler output before using it:
    exit code zero with an empty call graph provides no stack attribution.
    See the [native control investigation](../docs-dev/sog-repeatability-evidence.md).

## Loading and LOD stress scenarios

Beyond full-file loading, use a representative multi-source hierarchy to replay
budget pressure, turn-away/return, teleport while work is pending, cancellation
during decode/upload and budget changes in flight. Record coverage, time to settle,
queue/in-flight/pinned bytes, wasted work and frame tails. Empty queues alone do
not prove that uploads or visible refinement finished.

Current automated CPU fixtures validate spatial selection, both source budget
domains, covered relocation, eviction and resident scheduling. The browser harness
supports real hierarchies and constrained settings; its generated single-leaf
hierarchy is a smoke input. The native GPU harness renders complete files.
Large-scene stress coverage and a fair SuperSplat comparison require the same real
asset and matched cameras. The reference benchmark checkout contains no large model
payloads, so generated fixtures cannot establish that comparison.

Streamed SOG already provides chunk loading and LOD refinement. Standalone local
PLY/bundled SOG loading uses a separate whole-file path. Prioritized measurements,
stress replays and optimization candidates are tracked in the
[performance TODO](../docs-dev/splat-performance-todo.md).

!!! note "Mobile coverage"
    Current replay evidence is desktop-only: the browser harness fixes a 1000x760
    viewport and DPR 1. Recording browser identity or emulating touch does not
    establish physical mobile performance. The reported mobile/SuperSplat gap is
    a P0 follow-up requiring matched phone, asset, backend, physical pixels and
    quality, plus sustained runs and memory/input-stall observations.

## Audited revisions

The audit used these source revisions, not an assumption about current upstream
behavior:

| Repository | Revision |
| --- | --- |
| `3dgs-renderer-bench` | `7815826c9e16b9f83c0a5a0aab5f4d3a21dcfd53` |
| `spark` | `ffe210dc0a45d0515a06c9235878108112827d61` |
| `engine` | `46233e5bcd2ab17395f242341bc3be480188ec23` |
| `supersplat` | `b89029dd20911fd18905b61de2701094ac390a73` |
| `Gaussian-Splat-Lite` | `3cd506ae21cfaf842501a80b1c443af2799333b0` |
