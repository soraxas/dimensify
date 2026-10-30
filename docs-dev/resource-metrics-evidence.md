# Resource collector validation (2026-10-29/30)

The collector producer is clean revision `05b719f`. This establishes new
instrumented histories, not an optimization comparison with older collectors.
Raw samples, binaries and images remain ignored under
`dist/resource-metrics-20261029/`.

## Native measurements

Three fresh optimized processes each measured Apartment, JunkShop SH3 and an
explicit HDR encoded-compositing Apartment workload on Apple M5 Max / Metal.
Each process measured four camera cases, at least 240 completed frames and two
seconds per case. The fixed interior camera JSON files remain the workload
inputs. Default rendering uses the same static pixels as the earlier collector:
all 12 static capture checks match the prior producer exactly.

Median of three process observations, in MiB:

| Workload | OS peak RSS | RSS after decode | Decoded vector capacity | Tracked GPU buffers after main static |
| --- | --- | --- | --- | --- |
| Apartment | 667.05 | 254.27 | 151.40 | 162.14 |
| JunkShop SH3 | 919.56 | 349.73 | 228.89 | 245.12 |
| Apartment encoded | 667.33 | 254.31 | 151.40 | 162.14 |

These columns overlap and must not be added. OS peak includes the entire
process lifetime and captures. The tracked-buffer inventory excludes render
targets and other GPU allocations, so its identical linear/encoded value does
not mean encoded compositing has no additional memory cost.

All measured native frames have valid raster samples. Encoded workloads also
have complete encode/decode sample coverage. Flat-cloud workloads correctly
have no compact projection samples. Scopes retain independent frame IDs,
coverage and distributions; no total GPU frame time is inferred.

## Validation boundaries

Verification passed: 230 regular splat tests, 22 explicit offscreen GPU
correctness cases, 21 native runner tests, native Clippy, script/worker tests,
workflow checks, formatting and an optimized production WASM viewer/codec build.
The additional vendor-local ignored timestamp probe was not run; real-scene
pass coverage and pixel equality validate the exercised native paths.

## Browser LOD and publication

Three fresh browser contexts each ran a cold load and warm replay for Playbot
at default and constrained budgets: 12 successful replays, with no browser
errors. The active WebGPU identity remains privacy-redacted, so both reports
use `browser-diagnostics` and cannot establish a hardware regression gate.
The local server excludes WAN latency.

Median of the three observed round peaks:

| Budget | Main WASM capacity | Largest single worker WASM capacity | Completed worker jobs | Failed jobs |
| --- | --- | --- | --- | --- |
| Default | 93,782,016 bytes | 7,274,496 bytes | 7 | 0 |
| Constrained | 71,565,312 bytes | 7,077,888 bytes | 5 | 0 |

Worker decode wall sums were 149.8/46.3 ms for default/constrained; round-trip
sums were 433.5/275.4 ms. These cover observed jobs across the cold+warm replay,
not individual frame times or pure CPU consumption. The budgets perform
different amounts of work, so lower constrained numbers are not an optimization
comparison. Approximate optional V8 heap counters are also retained.

Five service reports contain 590 measurements, validated against saved raw
observations, fixture hashes, executable/build identities and returned values:

- [Apartment: 92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a187c5-c0f2-72d5-b5b2-cbefd7aabb0a)
- [JunkShop: 92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a187c5-cb38-7219-a67e-da7ddf26a596)
- [Apartment encoded: 116 metrics](https://bencher.dev/console/projects/dimensify/reports/01a187c5-d730-77a0-9541-b86f13944295)
- [Playbot default: 145 diagnostic metrics](https://bencher.dev/console/projects/dimensify/reports/01a187c5-ed3f-7099-bbda-875122fde324)
- [Playbot constrained: 145 diagnostic metrics](https://bencher.dev/console/projects/dimensify/reports/01a187c5-f795-76a7-a80d-8880f5d3f032)

The uploader also handles Bencher's repeated-dash slug normalization after a
readable display rename. A failing-first test verifies the stable alias while
rejecting collisions; all 57 performance helper tests pass after the fix.

Nine new plots contain 32 readable series, each preflighted against its exact
branch/testbed/benchmark/measure dimensions and saved measurement value. All
13 prior plot UUIDs, order and selections remain unchanged. All five reports
were re-fetched and all 590 values revalidated after readable-label updates.
Plot configuration and data were API-verified; the service dashboard layout
was not visually reviewed. See [plot interpretation](../docs/performance-plots.md).

See [measurement definitions](../docs/performance-resources.md). Total GPU
residency, whole-frame GPU time, pure worker CPU, isolated queue/transfer costs,
worker heap and browser RSS remain unavailable. Physical mobile hardware still
needs a separate recorded testbed. Hosted CPU regression gates are unchanged;
hardware real-scene runs use the trusted maintainer-dispatch workflow.
