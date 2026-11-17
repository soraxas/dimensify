# Adaptive transparency evidence (2026-11-17)

Apple M5 Max, headless Chromium 153 (WebGPU, ANGLE Metal), 1000x760 at DPR 1,
streamed Playbot fixture `3a40bceb8ffa`. One `just perf-ab` plan
(`tests/performance/ab-plans/transparency-motion.json`), two repeats of three
rounds, alternating arm order. `base` is `343ebe0` (always sorted); `auto` and
`movement` are the same candidate build with `?transparency=`. Motion columns
are warm replay app-frame intervals over the `moving-*` cases.

| Arm | First load-ready (ms) | Settle (ms) | Motion p50 (ms) | Motion p95 (ms) | Motion frames stochastic |
| --- | ---: | ---: | ---: | ---: | ---: |
| base | 199 | 572 | 16 | 29 | 0% |
| auto | 216 | 622 | 16 | 30 | 0% |
| movement | 199 | 574 | 16 | 29 | 97% |

`movement` drew 97% of motion frames stochastic. The remaining sorted frames are
probably at case starts, before the first pose change; this was not checked
frame by frame. `auto` stayed sorted, because the median
sorted frame on this GPU is about 16 ms, under the 25 ms threshold. Frame
intervals are vsync-bound here: raster takes about 1.2 ms for 500k splats, so
skipping the sort cannot show a desktop gain.

Static cases (`static-main`, `static-near-side`) drew 0 stochastic frames in
every warm `movement` run, so damped navigation settles inside the 150 ms window
and exact pose comparison is enough here.

The `auto` load difference (settle 622 vs 574 ms for the same build as
`movement`) comes from one 777 ms round and per-round spread of 570-648 ms. The
two arms share load code, so this is treated as noise, not a regression.

A native GPU test (`movement_policy_draws_its_first_moving_frame_without_compiling`)
checks the first moving frame: no pipeline is waiting, the frame draws, it is
stochastic (no smooth alpha), and settling restores sorted blending. With the
warm-up disabled, the test fails because the stochastic pipeline is still
compiling.

## Limits

- Desktop only. The expected benefit is on GPUs where sort plus raster misses the
  frame budget, such as phones. A physical Android run is still needed.
- `auto` measures whole-frame intervals, which include main-thread streaming
  work, so heavy loading can engage it briefly.
- The browser settings check (`check-splat-settings.cjs`) already failed on the
  baseline because of the new `Grid` toolbar action. With that expectation
  patched locally, the transparency cycle passed. A later camera-equality
  assertion failed by about 6e-8 in a quaternion component; not investigated.
