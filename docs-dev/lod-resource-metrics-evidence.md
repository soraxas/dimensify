# Packed LOD resource validation (2026-10-30)

Clean producer `aff3c12` exercised the prepared inlier-data Playbot hierarchy,
asset set `3a40bceb8ffa`. All 57 files were hash-verified before and after each
suite. Models, captures, binaries and raw samples remain outside Git under
`dist/lod-resource-metrics-20261030/`.

## Native packed streaming

Three fresh optimized processes per preset ran static, moving and
near/far/return phases through production `SogStream`, disk resolution and
packed projection caching at 1280x720. Every phase measured at least 240
completed frames and two seconds of completed work. The six processes retained
11,040 frames with actual projected-draw acknowledgements and complete valid
coverage of projection, radix, raster and both encoded-compositing scopes.

Each phase's capture hash matched across its three repeats within a preset.
All six phase/preset captures were visually inspected and show the front of
Playbot. These bounds-fitted workloads are not approved Pixel Eagle references
or proof of cross-engine parity. The browser uses a different camera protocol.

Median of three process observations:

| Native measurement | Default | Constrained |
| --- | ---: | ---: |
| Moving completed-frame median | 4.016 ms | 3.185 ms |
| Moving completed-frame P95 | 5.611 ms | 4.815 ms |
| Moving projection GPU median | 24.375 us | 20.042 us |
| Whole-process OS peak RSS | 357,793,792 bytes | 350,076,928 bytes |
| Tracked buffer capacity after return | 63,702,400 bytes | 11,402,368 bytes |
| Metal resource allocation after return | 177,029,120 bytes | 99,794,944 bytes |
| Near/far phase median displayed splats | 250,000 | 125,000 |

Different quality budgets perform different work; these values are not an
optimization comparison. Completed-frame cost includes CPU and GPU waiting,
not display FPS. Pass scopes are never summed into GPU frame time. The Metal
counter passed an exact 1 MiB active-wgpu allocation probe in every process;
it is API resource allocation, not physical VRAM residency. The three memory
rows overlap and must not be added. Hash verification warms the OS file cache.

## Continuous browser and worker diagnostics

Default and constrained suites each completed three fresh browser processes
with cold/warm replays and no browser errors. Browser v4 retains originating
GPU sample IDs, sparse-coverage counters and deduplicated allocation inventories.
The active WebGPU adapter remains privacy-redacted, so both suites publish to
`browser-diagnostics`, independently of native hardware histories.

| Observed browser measurement | Default | Constrained |
| --- | ---: | ---: |
| Raster GPU median | 2.102 ms | 1.411 ms |
| Raster GPU P95 | 2.521 ms | 1.639 ms |
| Buffer capacity peak | 30,762,624 bytes | 6,409,344 bytes |
| Packed source payload peak | 29,328,208 bytes | 7,095,280 bytes |
| Main WASM capacity peak | 94,044,160 bytes | 71,761,920 bytes |

These are observed GPU distributions and median round peaks, not complete
frame GPU timing or total memory. Browser projection and encoded compositing
are inactive in this workload and have no invented latency values.

An independent three-round worker-profile suite observed seven workers and
seven retained segments per round, without protocol errors, flush timeouts or
retention-limit failures. Each round also retains seven unflushed termination
tails. Its median sampled elapsed attribution is 307,283,000 ns across 242
samples; it includes idle/GC and is partial, not exact worker CPU consumption.
Only profile metrics enter `worker-profiles`; its perturbed frame/loading
measurements are excluded. Contradictory profiling signals are rejected.

## Published history and verification

All 883 submitted measurements were re-fetched and checked against saved BMF
values, raw samples, producer and complete fixture identities:

- [Native default: 222 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1881b-67eb-704f-984e-86263be8fc49)
- [Native constrained: 222 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1881b-7c0f-7659-851b-22f7960bb11c)
- [Browser default: 214 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1881b-8e33-77ba-a57b-54fee19268e4)
- [Browser constrained: 215 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1881b-9f24-746a-96b3-50ce146bedef)
- [Worker profiles: 10 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1881b-b006-74f7-ada8-2d0a8f8d4c16)

Eleven new plots contain 37 readable series, with exact dimension/value
preflights. All 22 earlier plot configurations and ordering remain unchanged.
All five reports were re-fetched and all values revalidated after renaming.
Plot configuration/data were API-verified; dashboard layout was not visually
reviewed. See [plot interpretation](../docs/performance-plots.md).

Verification passed: 233 regular splat tests, 22 existing GPU correctness cases,
the new actual-GPU compact-owner retirement test, 21 flat and 14 LOD runner
tests, native Clippy, optimized WASM viewer/codec builds, 117 Python tests,
script/worker suites, workflow lint and formatting. The real headless worker
and device-helper suite passes all 11 cases. Review fixes ensure actual
projected draws and live render-world owners are recorded.

The user selected desktop/headless support first. Physical mobile replay,
adaptive phone input targeting and real-device measurements remain TODOs.
Exact worker CPU, portable residency, full-frame GPU time, worker JS heap and
browser process RSS remain outside this collector's coverage. These reports
establish histories; no regression-gate or renderer-speedup claim is made.
