# Real-scene benchmark evidence (2026-10-29)

The initial six verified reports record 548 metrics in the
[Bencher project](https://bencher.dev/perf/dimensify). Each measurement is the
median of three independent runs; each run's percentiles remain separate in raw
results. These reports establish history, not a passed regression gate.
The [SOG buffer handoff follow-up](sog-buffer-handoff-evidence.md) adds matched
baseline/candidate measurements and exact static-pixel comparisons.

## Native interior camera candidates

Clean producer `d14bb26a713d2229f76ab368ac0117caf779d95c`, optimized bench
profile, Apple M5 Max / Metal, 1280 x 720, SH3, 32-bit radix sorting, no MSAA,
one frame in flight, offscreen completion and GPU timestamps enabled. Each
process measures four static/moving main/side cases, at least 240 completed
frames and two seconds per case. Builds completed before timing.

| Scene | Expanded decode (ms) | Main static completed-frame median / P95 (ms) | Main static GPU radix/compaction median (ms) | Report |
| --- | ---: | ---: | ---: | --- |
| Apartment, 661,466 splats | 75.639 | 5.552 / 7.045 | 0.914 | [40 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18741-ea6c-7454-90ef-78d17be1e544) |
| JunkShop SH3, 1,000,000 splats | 165.954 | 6.764 / 8.124 | 0.996 | [40 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18741-effd-76d7-a781-56cf9bd03231) |

Independent main-static frame medians were 5.871/5.552/5.552 ms for Apartment
and 6.779/6.764/6.676 ms for JunkShop. Completed-frame wall time includes CPU
work and synchronization; it is not display FPS. The GPU metric covers radix
sorting/compaction, not the complete render pipeline.

Camera JSON lives in `tests/performance/`; its hash separates workload identities.
All eight captures were inspected. Main views show the Apartment interior and
JunkShop counter. Apartment's side view is too close to geometry and needs
refinement before representative overdraw or cross-renderer comparisons.
No real-scene Pixel Eagle reference was approved. Earlier automatic bounds-fit
reports remain distinct and should not stand in for interior workloads.

## Browser loading and LOD observations

Clean producer `747332fb3e438bc4759996e706409b09f010b80e`, release instrumented
production viewer, separately packaged production SOG worker/codec, headless
Chromium 153.0.8010.12, 1000 x 760 canvas, DPR 1. Each scenario has three fresh
browser rounds, each with a cold load and a warm live-cache camera replay.
Assets are served locally; these results exclude WAN latency.

| Scenario | Cold loader ready (ms) | Cold renderer encoded ready (ms) | Cold refinement settled (ms) | Report |
| --- | ---: | ---: | ---: | --- |
| Apartment default | 232.7 | 322.3 | 337.6 | [111 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1873a-5374-7716-b394-a759e8c5c8af) |
| JunkShop SH3 default | 483.6 | 589.5 | 590.6 | [111 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1873b-5320-7309-a3a3-c90095d262ee) |
| Playbot LOD default | 183.0 | 195.9 | 1642.9 | [123 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18739-a6e8-714a-95b7-c808abc8734a) |
| Playbot LOD constrained | 160.0 | 172.8 | 1133.8 | [123 metrics](https://bencher.dev/console/projects/dimensify/reports/01a1873a-49f3-7418-b031-9b0d65bc67ae) |

The active WebGPU adapter name is privacy-redacted. Browser inventory reports
Apple M5 Max / Metal, but does not attest the active WebGPU device. Reports
therefore use `browser-diagnostics` and `observed-browser-v1-*` testbeds.
Normal hardware publication still rejects ambiguous identity. Diagnostic
publication rejects known software adapters and cannot use `--check`.

Default Playbot settled at 250,000 ready logical splats and reached an observed
500,000 during motion. Constrained Playbot settled/reached 125,000, with a 150k
requested splat budget, 32 MiB CPU/source budgets and one concurrent source.
Observed ready-cache payload peaks were 29,321,880 versus 7,090,760 bytes;
reserved-cache peaks were 16,785,408 versus 7,348,224 bytes. The constrained
run renders different quality, so faster settling is not an optimization result.
Sampled payload peaks are not RSS or VRAM measurements.

Reports also include per-camera RAF/encoded cadence percentiles, observed CPU
stage totals/maxima and ready/pending counts. CPU stages span asynchronous jobs
and do not measure worker CPU consumption. Encoded draw readiness is not GPU
completion or first visible pixel. Browser camera paths are not matched to the
native interior poses or other renderers.

## Reproduction and remaining coverage

Use the [real-scene commands](../docs/performance.md#repeat-and-publish-real-scene-measurements)
with externally prepared `fixture.json` inputs. Fixture bytes and all-set hashes
are checked before and after measurement. Raw results, captures, compact BMF,
full-name mappings and service responses remain in ignored
`dist/real-scenes-20261029/`, under:

- `apartment-v1-native-interior` and `junkshop-sh3-v1-native-interior`.
- `apartment-v1-browser-default-v2` and `junkshop-sh3-v1-browser-default-v2`.
- `playbot-lod-v1-browser-default-v2` and `playbot-lod-v1-browser-constrained-v2`.

No fixture binaries or rendered images are committed. Conflicting/unverified
asset permission evidence still blocks automatic acquisition/publication.
The manual hardware workflow requires a trusted provisioned runner; adding it
does not establish continuous GPU CI. Physical mobile, matched cross-renderer
comparisons, calibrated real-scene visual references, full GPU pass attribution,
worker CPU time and total memory remain open in the
[performance TODO](splat-performance-todo.md). Other desktop activity was not
attested idle, so these initial local runs are not controlled laboratory results.
