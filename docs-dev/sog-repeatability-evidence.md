# Gaussian timing repeatability investigation (2026-10-30)

The earlier [scale-codebook experiment](sog-scale-codebook-evidence.md) has mixed
frame tails. This follow-up establishes substantial timing variance without a
code change and narrows the historical spikes to application/submission and wait
intervals. It does **not** establish their exact cause or clear the regression.

## Identical-executable controls

Build clean main `f1f11489df23d636ee6b7716fe5b1a5294c10a2c` once with the optimized
bench profile. Its executable SHA-256 is
`5f000124225aaa344476a1efb07eb3a78e5df28439a80306d47d982874101def`.
Run three independent processes in each of two control groups per scene on the
same Apple M5 Max / Metal, 1280x720, SH3, Radix32 and interior camera fixtures.
Alternate A/B order each round and reverse the starting order for the other
scene. These groups contain the **same executable and revision**, not competing
implementations. Full recorded identities match, including the executable.
All experiment builds finish before measurement; desktop activity, clocks and
OS/driver caches remain uncontrolled.

Each process keeps the existing minimum of 240 completed frames and two seconds
of work per view, extending by complete camera cycles when necessary. Compare
medians of independent process summaries, not pooled frame samples.

| Scene / metric | Control A (ms) | Control B (ms) | Difference |
| --- | ---: | ---: | ---: |
| Apartment / one-shot decode | 62.840 | 66.130 | +5.2% |
| Apartment / main-static frame median | 6.617 | 7.244 | +9.5% |
| Apartment / side-moving frame P95 | 8.853 | 10.098 | +14.1% |
| JunkShop / one-shot decode | 170.763 | 285.233 | +67.0% |
| JunkShop / main-moving frame P95 | 9.228 | 11.202 | +21.4% |
| JunkShop / side-moving frame P95 | 9.841 | 9.638 | -2.1% |

These differences measure repeatability, not improvements or regressions between
revisions. They make small one-shot loading differences insufficient evidence of
a gain. They do not by themselves explain the earlier 136.7% side-moving P95
increase. All 24 static captures match exactly within each scene/view. These are
equality checks, not newly approved Pixel Eagle references.

## Historical spike attribution

Recheck the six original interleaved JunkShop side-moving processes. All 240
camera poses match bit-for-bit between processes, with one acknowledged sort and
draw per frame. Baseline runs contain 2 / 0 / 0 frames over 30 ms; candidate runs
contain 9 / 21 / 0. Thus the earlier P95 result is not a camera-sequence mismatch
or an omitted sort/draw.

In candidate round 2, the slowest frame is 88.532 ms: 68.719 ms inside
`App::update` and 19.813 ms in the subsequent completion wait. Its measured radix
pass is 0.994 ms and raster pass 4.687 ms. Across the two slow candidate runs,
completed-frame wall time correlates with app-update wall time at 0.944 / 0.916.
This localizes much of the extra wall interval outside the measured shader
durations; it does not prove pure CPU execution, a specific scheduler/driver
fault, or environmental interference. App update includes scheduling, submission
and internal waits. GPU work overlaps host work; pass times and percentile
distributions cannot be subtracted to invent exclusive CPU time.

Rerun the exact historical candidate executable, verified against its original
`3959d2b403d18d1859f6611ccf2f013cc7a669a5f50fd62011924ee4ad64a97f` hash.
Its unprofiled side-moving P95 is 9.619 ms across 480 samples; the first 240 poses
exactly match the original sequence and have P95 9.711 ms with no frames over
30 ms. The spike does not reproduce in this rerun. This is a smoke diagnostic:
the current source checkout differs from the embedded build revision, so it is
excluded from performance publication. A separate longer profiled diagnostic
is also excluded. The OS sampler returned an empty call graph despite exit code
zero; a live child-process inspection control failed too. No stack attribution
is available from that attempt.

## CPU-only expansion probe

Use a throwaway native harness with identical source and dependency locks for
codec snapshots at baseline `1c1cbe83443bc2da6adf164943012eb4730ba31b` and current
main. The manifests use opt-level 3, thin LTO and one codegen unit under Rust
1.98.1. Hash-verify the external fixtures and exported codec source files. Run
three fresh processes per revision/scene, alternating order; each warms twice
and measures 15 expansions. No GPU or renderer is initialized. Time the shared
production `decode_expanded` call with SH3, including WebP decode and output
allocation, with pre-extracted encoded inputs in memory. Exclude disk, ZIP,
metadata parsing, output hashing and output destruction. Repeated iterations
within one process are not independent trials.

| Scene | Baseline process medians (ms) | Current process medians (ms) | Median change |
| --- | --- | --- | ---: |
| Apartment | 51.172 / 51.196 / 48.393 | 46.297 / 45.639 / 47.633 | -9.5% |
| JunkShop | 140.028 / 137.261 / 134.480 | 134.654 / 133.089 / 130.529 | -3.0% |

Complete output hashes, including every float bit and plane length, match
between revisions and between each process's first/last expansion. These are
local warm expansion observations, not end-to-end loading, FPS, browser or phone
results. [Probe source, dependency lock and staging commands](../tools/perf/probes/sog-expansion/README.md)
are retained in the repository. The diagnostic harness is not a new maintained
Bencher workload.

## Publication and remaining work

Four main-branch Bencher history reports retain all 368 control metric values,
including unfavorable results. Saved-result validation recomputes the metrics,
checks fixture/capture identities and verifies returned measurements/revisions.
There are no returned alerts; no regression check, threshold change or baseline
reset is performed.

| Scene | Control A report | Control B report |
| --- | --- | --- |
| Apartment | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c6e-d2a3-76f1-abd2-a208e6000231) | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c6e-e02d-747b-b871-c0602ef8a297) |
| JunkShop | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c6e-ecbf-715a-8b80-6cd04e749b3f) | [92 metrics](https://bencher.dev/console/projects/dimensify/reports/01a18c6e-f8d8-74a1-b5f3-5d4f4545680b) |

Raw reports, staged codec inventories, binaries, execution order, captures,
attribution data and service responses remain locally in ignored
`dist/sog-repeatability-20261030T104158Z/`. The control driver and probe sources
also remain with that original local evidence. Generated results are not part of
the source changes.

Next, capture host execution/submission/wait traces while the slow condition is
actually occurring, with usable stack or scheduler attribution. Repeat on an
idle fixed GPU worker before setting GPU regression thresholds or making overall
performance claims. Preserve the original unfavorable reports and the unresolved
startup/tail follow-up. No rendering, loading or quality policy changes in this
investigation.
