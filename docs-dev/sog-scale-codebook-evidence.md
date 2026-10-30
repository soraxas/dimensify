# Expanded SOG scale-codebook experiment (2026-10-30)

Baseline: `1c1cbe83443bc2da6adf164943012eb4730ba31b` (main). Candidate: `ba1a8a7055201b777c17c81009b743d60d557308` (`perf/sog-scale-codebook`).

The expanded SOG decoder resolves the byte-indexed log-scale palette once per source: at most 256 exponentials replace three exponentials per authored splat. Each entry retains its original missing/non-finite/invalid-exponent error, reported only when referenced and with the original splat/axis context. Packed LOD already validates its scale palette separately.

The same shared codec serves native expansion and browser workers. These measurements cover native bundled-SOG decoding; browser and physical-mobile performance remain unmeasured for this change.

!!! note "Repeatability follow-up"
    [Identical-executable controls](sog-repeatability-evidence.md) subsequently
    show substantial variance, including 67.0% in JunkShop one-shot decode medians.
    Keep the percentages below as observations, not established loading gains.
    A separate warm CPU-only expansion probe narrows the scope; the frame-spike
    cause remains unresolved.

## Method

Both clean optimized bench binaries were built before timing. Each batch ran three independent processes per revision and scene on Apple M5 Max / Metal, 1280x720, SH3, Radix32, no MSAA and one frame in flight. Use the hash-verified external Apartment/JunkShop fixtures and their checked-in interior camera candidates. Scoped radix/raster timestamps, startup, CPU/process memory and completed-frame samples are retained separately. Each view requires at least 240 completed frames and two seconds of completed work.

The first batch grouped three processes per revision: Apartment baseline/candidate, then JunkShop candidate/baseline. Frame variability prompted a second batch that alternated revision order per round: Apartment AB/BA/AB and JunkShop BA/AB/BA. Builds stayed idle throughout measurement. OS/driver caches were not reset; other desktop activity and device clocks were not controlled. Executable hashes differ as expected and are individually verified; all remaining hardware/toolchain/render/protocol identity fields match.

## Decode-stage observations

| Batch | Scene | Baseline median (ms) | Candidate median (ms) | Change |
| --- | --- | ---: | ---: | ---: |
| Grouped | apartment-v1 | 75.477 | 74.492 | -1.3% |
| Grouped | junkshop-sh3-v1 | 222.016 | 194.718 | -12.3% |
| Interleaved | apartment-v1 | 79.518 | 73.884 | -7.1% |
| Interleaved | junkshop-sh3-v1 | 238.285 | 214.975 | -9.8% |

Decode is the production bundled-SOG decode/validation stage, excluding input reading, renderer initialization and first draw. Candidate medians are lower in both batches, but independent ranges overlap. This is a local stage observation, not a proven end-to-end loading, frame-rate or cross-renderer improvement.

Raw independent decode samples (milliseconds):

- Grouped, apartment-v1: baseline 75.477 / 72.597 / 81.353; candidate 74.492 / 71.586 / 76.566.
- Grouped, junkshop-sh3-v1: baseline 184.720 / 222.016 / 230.701; candidate 194.718 / 199.446 / 191.063.
- Interleaved, apartment-v1: baseline 91.594 / 79.518 / 75.264; candidate 94.351 / 73.884 / 68.160.
- Interleaved, junkshop-sh3-v1: baseline 238.285 / 298.451 / 174.480; candidate 161.028 / 214.975 / 224.530.

## Frame and startup limits

The grouped Apartment main-static frame median rose 19.0% and its P99 rose 41.6%. In the interleaved repeat its median difference was 0.3%, while other views still show substantial mixed changes. The interleaved JunkShop side-moving P95 rose 136.7%, including candidate P95 values of 27.590 / 35.011 / 11.354 ms. These observations remain in Bencher and raw evidence; no overall rendering improvement or passed GPU regression gate is claimed. Controlled same-binary repeats and stage isolation are follow-up work.

Interleaved completed-frame medians and P95 (median of each process percentile, milliseconds):

| Scene / view | Baseline median / P95 | Candidate median / P95 |
| --- | ---: | ---: |
| apartment-v1 / main-static | 8.114 / 9.913 | 8.141 / 10.389 |
| apartment-v1 / main-moving | 7.259 / 9.190 | 9.291 / 12.624 |
| apartment-v1 / side-static | 6.503 / 9.159 | 7.518 / 9.834 |
| apartment-v1 / side-moving | 7.116 / 9.109 | 6.860 / 9.019 |
| junkshop-sh3-v1 / main-static | 12.748 / 22.814 | 12.004 / 14.782 |
| junkshop-sh3-v1 / main-moving | 11.196 / 17.067 | 10.330 / 12.384 |
| junkshop-sh3-v1 / side-static | 10.719 / 13.457 | 9.813 / 11.638 |
| junkshop-sh3-v1 / side-moving | 9.755 / 11.658 | 11.573 / 27.590 |

| Interleaved scene | First completed draw after spawn, baseline / candidate (ms) | Process peak RSS, baseline / candidate (MiB) |
| --- | ---: | ---: |
| apartment-v1 | 328.548 / 302.935 | 657.1 / 668.2 |
| junkshop-sh3-v1 | 577.831 / 509.093 | 907.5 / 913.5 |

First-draw differences change direction between batches and driver/pipeline cache state was not reset. Process peak RSS differs by less than 2% in these repeats; this is not GPU residency or total VRAM accounting.

## Correctness and publication

All 48 static captures (two scenes, two static views, two revisions, three processes, two batches) are byte-identical across matching scene/view comparisons. Moving captures are retained but not compared at unequal final animation frames. The existing camera candidates remain unapproved as visual references; this experiment does not establish a reviewed Pixel Eagle baseline.

Validation passed: 231 regular splat tests and six codec tests, strict codec Clippy, codec formatting and WASM worker compilation. The 44 ignored GPU/external cases were not counted as executed. New codec tests compare every scale index bit-for-bit and cover missing, invalid, unused and zero-count entries. An eager-rejection mutation fails the unused-entry test and was restored. Read-only code review found no blocking issues.

CI explicitly selects the codec for formatting, headless tests/lint and WASM worker Clippy. The new scoped worker Clippy command and codec tests also pass locally; compiling the codec only as another package's dependency would not execute these tests.

Eight Bencher reports retain all 736 metric observations, including adverse frame/startup measurements. Upload validation recomputes metrics from raw samples and verifies returned metric values, source revisions, measures and report inventories. These are history publications without creating, resetting or relaxing thresholds.

| Batch | Scene | Baseline report | Candidate report |
| --- | --- | --- | --- |
| Grouped | apartment-v1 | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c3f-5642-7203-bc1e-c9239a673391) | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c3f-6273-7457-96c6-cb72427968dc) |
| Grouped | junkshop-sh3-v1 | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c3f-789b-7188-80b5-5a73cf708013) | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c3f-6da3-753c-99bb-09ed9affc215) |
| Interleaved | apartment-v1 | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c44-5024-752d-80e2-c93c20fe5128) | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c44-5c5d-77ee-af89-2be657160a69) |
| Interleaved | junkshop-sh3-v1 | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c44-705e-7036-851a-2af295562916) | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c44-66c3-772c-9f75-cc96224b9189) |

Raw JSON, executable hashes, captures, process usage, comparison summaries and service responses remain in ignored `dist/sog-scale-codebook-20261030T095109Z/`, with the repeated batch in `interleaved/`. The two batches use the same recorded binaries and matched fixture/camera identities.
