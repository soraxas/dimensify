# Native GPU verification (2026-10-28)

Clean optimized build and runtime commit:
`c25a9c0c789408defdd74ef27037b705c7abc3dd`.

Command: `mise exec -- just perf-gpu --output dist/performance-gpu-clean-c25a9c0`.
Hardware: Apple M5 Max, Metal, Darwin arm64. Rust 1.98.1, opt-level 3,
1280x720 offscreen, generated 65,536-splat SH3 PLY, normalized Gaussian raster,
32-bit radix sort. Optional GPU timestamps disabled.

| Case | Completed samples | Median wall time (ms) |
| --- | --- | --- |
| Main static | 480 | 5.018 |
| Main moving | 480 | 7.778 |
| Near-side static | 240 | 11.855 |
| Near-side moving | 240 | 12.154 |

All 1,440 measured frames acknowledged real sorting and drawing. Every case
completed at least two seconds of measured work and produced a nonempty capture.
`raw.json` contains exact executable/source hashes, poses, raw intervals and
percentiles; `bmf.json` contains 24 latency metrics. The publication guard accepted
the clean matching optimized hardware run. Results remain local, without upload.

These are serialized completed-frame wall times, including app update and GPU
waiting, not display FPS or isolated GPU execution time. One generated asset and
one run establish runner acceptance; they do not establish large-scene performance
or a statistical regression baseline. The near-side view increases projected
coverage and overdraw. Supply real scenes and repeat matched runs on stable
hardware before drawing renderer comparisons.

Earlier ci-profile smoke evidence is in
`/tmp/dimensify-native-gpu-smoke-20261028`: 400 acknowledged frames, each matched
to a valid optional radix timestamp, plus four inspected captures. That dirty,
unoptimized run correctly suppressed BMF output. The optimized run above uses
the default timestamp-disabled protocol and must not be mixed with it as history.
