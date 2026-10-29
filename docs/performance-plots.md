# Reading the performance dashboard

Open the [Dimensify performance dashboard](https://bencher.dev/perf/dimensify)
for the curated pinned plots. **Settings > Dimensions > Benchmarks** is the raw
inventory; it includes detailed diagnostics and unused entries from failed
publications, rather than a recommended set of charts.

The dashboard has 22 plots. Each uses one measure and a linear value axis, with
commit version on the horizontal axis and a rolling 90-day window. A percentile
has its own series; it is not drawn as a confidence interval. Real-scene points
aggregate three independent runs. Native plots include matched baseline and
candidate commits for the [SOG buffer handoff](../docs-dev/sog-buffer-handoff-evidence.md).
Browser scene plots still establish initial observations.

| Plot | What to look for |
| --- | --- |
| Native completed frames: median | Typical serialized completed-frame cost for static/moving main interior views of Apartment and JunkShop |
| Native completed frames: P95 | Tail completed-frame cost for those same views |
| Native GPU radix + compaction: moving main | Median/P95 GPU sorting and compaction cost during camera motion |
| Native startup: decode and first completed draw | Production bundled SOG decode/validation and the separate first-completed-draw milestone after spawning |
| Browser bundles: cold loader and encoded readiness | Cold Apartment/JunkShop loading and renderer encoding milestones |
| Playbot LOD: cold readiness and settling (observed) | Loader/encoded readiness and refinement settling under default/constrained budgets |
| Playbot motion: warm encoded cadence (observed) | Median/P95 renderer-encoded intervals during warm-cache main-camera motion |
| Playbot LOD: sampled cache payload peaks (bytes) | Ready/reserved cache payload peaks under both budgets |
| Playbot LOD: sampled ready splats (quality context) | The quality/workload context needed to interpret the timing and cache plots |
| Playbot processing: replay CPU elapsed stages | Whole-replay decode and upload-preparation elapsed totals |
| CPU depth sorting: 1M splats by runner CPU | Criterion sorting estimates, with hosted CPU models kept separate |
| CPU SOG decode: 65k splats by runner CPU | Criterion decoding estimates on each hosted CPU model |
| CPU LOD selection: 1k and 10k leaves (EPYC 9V74) | Selection microbenchmark changes on one CPU model |
| Native memory: OS peak RSS and RSS after decode | Entire-process high-water memory versus the post-decode checkpoint |
| Native decoded Gaussian vector capacity | Reserved decoded attribute storage, excluding allocator metadata and decoder temporaries |
| Native tracked GPU buffers: capacity, excludes textures | Deduplicated flat-cloud, sorting and radix buffer capacities; not total VRAM |
| Native Gaussian raster: moving main, instrumented | Median/P95 actual raster timestamps for the new instrumented native cohort |
| Native encoded compositing: moving main GPU passes | Independent encode/decode GPU pass median/P95; never a summed GPU frame total |
| Playbot LOD: observed WASM capacity peaks | Main and largest single-worker WASM capacity under default/constrained budgets |
| Playbot LOD: approximate observed JS heap peaks | Optional V8 used/total heap counters; excludes worker heap and process RSS |
| Playbot workers: observed decode and round-trip wall sums | Whole-replay observed jobs; compare job coverage and quality alongside elapsed sums |
| Native CPU accounting: entire benchmark process | OS user/system CPU time for all four cases, startup and captures; not per-frame CPU |

!!! note "Compare equivalent work"
    Default Playbot settles at 250k ready splats; constrained settles at 125k.
    Their observed peaks during motion are 500k and 125k. Faster settling with
    less loaded detail does not establish a renderer optimization. Read the
    ready-splat plot alongside the timing and cache plots.

Native completion includes CPU work and waiting for the GPU; it is not display
FPS. GPU timestamps cover eligible independent passes, not whole-frame GPU time. Native main
views are candidate interior workloads; the problematic Apartment side view is
excluded from these pinned plots.

Browser results use `browser-diagnostics` because the active WebGPU identity is
redacted. Encoded cadence is not GPU completion. Assets are served locally, so
loading excludes WAN latency. Sampled cache payload bytes are not total RSS or
VRAM. The asynchronous CPU stage totals are elapsed wall time, not worker CPU
consumption or additive per-frame timings. These plots do not measure phones.

The original 40 real-scene benchmarks and 32 new resource series have readable
display labels. Their slugs and UUIDs remain unchanged, preserving measurements
and upload identity. Saved reports are validated against those stable aliases,
so a display-name change does not break result verification.

Nine additional resource plots use readable series ending in `r1` for the
[resource collector cohort](../docs-dev/resource-metrics-evidence.md). They
establish initial measurements from one producer revision, with three process
runs or browser rounds per point. The earlier 13 plots retain their UUIDs,
order and workload selections. New instrumentation has separate histories;
do not splice these timings into the earlier optimization comparison.

Pinned plots select explicit benchmark/testbed UUIDs. New measurements for those
same workloads appear automatically. A new scene, camera, workload protocol or
v2 workload creates different series; review and update the pins explicitly
instead of joining incompatible histories. Adding more raw metrics does not
automatically add lines to these plots.

The native plots now select v2 workloads on the shared
`m5-max-metal-macos-25-6-0-arm64` testbed. Readable metric labels end in `v2`;
earlier v1 reports remain separate. The decode-stage reduction does not establish
faster overall startup or display FPS; inspect the separate first-draw and
frame-tail series too.

See [performance tracking](performance.md) for collection/publication commands
and the [real-scene evidence](../docs-dev/real-scene-benchmark-evidence.md) for
source revisions, reports and measurement settings.
