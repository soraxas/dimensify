# Expanded SOG buffer handoff (2026-10-29)

Candidate `0687b8bd0b5c62e939de53c1f3fc6491ce8e93f5` transfers owned decoded
attribute buffers into renderer planes with `bytemuck::allocation::try_cast_vec`.
This checks allocation alignment, length and capacity. Incompatible capacities
retain the previous copy behavior; mismatched plane lengths still panic.
Attribute values, padding, SH layout and the public decoder API are unchanged.

For one million SH3 splats, this removes four duplicate allocations and about
240 MB of attribute copying, including padding. This is a buffer-size calculation,
not a measurement of process RSS or GPU memory. Packed LOD decoding and streamed
browser worker readback use separate paths and are unchanged.

## Matched native measurements

Baseline `d14bb26a713d2229f76ab368ac0117caf779d95c`, candidate above. Both are
clean, optimized bench builds on Apple M5 Max / Metal, 1280 x 720, SH3, Radix32,
no MSAA, one frame in flight and radix/compaction GPU timestamps enabled.
Use the checked-in Apartment/JunkShop interior cameras and externally prepared,
hash-verified fixtures. Each point aggregates three independent processes, each
with four views and at least 240 completed frames and two seconds per view.
Builds finished before timing. Apartment ran baseline then candidate; JunkShop
ran candidate then baseline. Other desktop activity was not attested idle.

The decode stage includes production bundled SOG decoding and validation. It
excludes input reading, renderer setup and the subsequent first completed draw.

| Scene | Baseline decode (ms) | Candidate decode (ms) | Reduction |
| --- | ---: | ---: | ---: |
| Apartment, 661,466 splats | 77.262 | 62.109 | 19.6% |
| JunkShop SH3, 1,000,000 splats | 165.399 | 141.172 | 14.6% |

Independent decode samples, in milliseconds:

- Apartment baseline: 96.279 / 73.282 / 77.262; candidate: 62.109 / 64.674 / 61.390.
- JunkShop baseline: 165.399 / 181.227 / 155.198; candidate: 141.172 / 136.923 / 147.672.

Every candidate decode sample is below every baseline sample for its scene.
This establishes a local decode-stage improvement, not a SuperSplat comparison,
physical-mobile result or end-to-end startup/FPS improvement.

| Completed-frame view | Baseline median / P95 (ms) | Candidate median / P95 (ms) |
| --- | ---: | ---: |
| Apartment main static | 5.291 / 5.869 | 5.281 / 5.820 |
| Apartment main moving | 5.116 / 5.592 | 5.231 / 6.278 |
| JunkShop main static | 6.970 / 8.848 | 6.642 / 7.791 |
| JunkShop main moving | 6.712 / 8.260 | 6.687 / 7.906 |

Frame differences are retained without attributing them to this decoder change.
Apartment moving P95 increases 12.3%; its independent ranges overlap
(baseline 5.573-7.289 ms, candidate 5.877-7.547 ms). Native completion includes
CPU work and GPU waiting; it is not display FPS.

First-draw medians after spawning increase from 149.132 to 164.320 ms for
Apartment and 178.237 to 187.175 ms for JunkShop. Apartment's first baseline
process takes 3131.747 ms, followed by 148.109 and 149.132 ms; candidate samples
are 173.378 / 164.320 / 155.437 ms. Driver/pipeline cache state was not reset.
Startup and tail-frame behavior need further controlled repeats; no startup or
frame-rate improvement, or passed regression gate, is claimed.

## Correctness and publication

The failing-first regression detects the original redundant allocations.
All 230 regular tests pass, including exact bytes (signed zero and NaN payloads),
allocation reuse, incompatible-capacity fallback and malformed-plane rejection.
The reviewed 22 explicit offscreen GPU cases pass. The production instrumented
WASM viewer builds in release mode. Native main/side static captures are exactly
identical across all three processes and both revisions for each scene.
Moving captures are not compared at unequal final animation frames.
Existing real-scene camera limitations remain; these are not newly approved
Pixel Eagle references.

All 160 measurements are published and validated against their saved raw samples:

| Scene | Baseline report | Candidate report |
| --- | --- | --- |
| Apartment | [40 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18795-b7cd-72ab-aa16-66be0bfdf0e3) | [40 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18796-1023-72b4-a414-4e1dc33cf591) |
| JunkShop | [40 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18796-3e23-7260-8ff6-d3963bdbc925) | [40 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18796-9398-70c2-88e3-78c16d9ac51a) |

These v2 workload series share the readable
`m5-max-metal-macos-25-6-0-arm64` hardware testbed. The four native dashboard
plots select both measured revisions through the new series. Earlier v1 reports
remain separate. Initial history is uploaded without creating or relaxing
thresholds.

Raw samples, captures, executable hashes, source identities, exact-pixel checks,
benchmark mappings and Bencher responses remain in ignored
`dist/sog-handoff-20261029/`. No scene binaries or rendered images enter Git.
Reproduce with the [real-scene commands](../docs/performance.md#repeat-and-publish-real-scene-measurements),
using each clean producer checkout with its matching optimized executable.
