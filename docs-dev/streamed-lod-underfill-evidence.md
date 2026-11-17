# Streamed LOD underfill and codec worker evidence (2026-11-16)

Apple M5 Max, macOS 26.7.1, headless Chromium 153.0.8010.12 (WebGPU, ANGLE
Metal), 1000x760 canvas at DPR 1. The fixture is the prepared inlier-data Playbot
hierarchy, asset set `3a40bceb8ffa`: 7 LOD files, two leaves sharing one file per
level, 500,000 splats at LOD 0 (8.3 MiB) and 1,873 at LOD 6 (76 KiB). Assets
are served over localhost. Each arm runs three fresh browser rounds per process;
`just perf-ab` repeats each condition and alternates arm order. Values are the
medians of per-run cold medians. Threshold crossings are quantized by the 100 ms
telemetry cadence. Raw reports, logs and captures are in ignored `dist/ab/` and
`dist/bench-stream-*`. Nothing was published to Bencher.

## Baseline timeline

Before this change, streaming exposed only the next finer level after a resident
level (`next_files`). A cold load refined through 6 -> 5 -> 4 -> 3 -> 2 -> 1 -> 0
one file at a time. There was never more than one download in flight, despite
`concurrent_chunks: 2`. Each step cost a scheduling pass, the download, a fresh
worker and admission, about 250 ms per level. About 17 MiB was fetched to
display the 8.3 MiB target. Over seven jobs, worker dispatch-to-handler
(startup) took 331 ms of the 582 ms worker round trip. Decode itself took about
150 ms.

PlayCanvas's `lodUnderfillLimit` defaults to 0 (request the optimal LOD
directly), and SuperSplat does not override it. The engine's streaming examples
set 5-10, which steps like the old Dimensify behavior.

## Underfill window

`SogLodSettings::lod_underfill_limit` uses the same semantics. `None` keeps
stepping. `Some(n)` requests the coarsest level within `n` steps of each leaf's
target, then steps finer inside that window.

| Condition | Arm | Settle (ms) | 31k ready (ms) | 125k ready (ms) | Decode jobs | Cache peak (MiB) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Localhost | stepping (two runs) | 2111 / 1922 | 1027 / 955 | 1354 / 1252 | 7 | 28 |
| Localhost | underfill 0 | 777 | 777 | 777 | 3 | 21 |
| Localhost | underfill 2 | 1233 | 590 | 590 | 4 | 26 |
| 50 Mbps, 20 ms | stepping | 5464 | 1737 | 2523 | 7 | 28 |
| 50 Mbps, 20 ms | underfill 0 | 2898 | 2898 | 2898 | 3 | 21 |
| Constrained | stepping | 2580 | 1921 | 2580 | 6 | 14 |
| Constrained | underfill 0 | 1296 | 1296 | 1296 | 3 | 13 |

The first matrix ran each arm once, as one process of three rounds. A second
plan with two repeats measured localhost underfill 0/2 at 1001/1284 ms settle and
50 Mbps underfill 0/2 at 2254/4192 ms. Under throttling, underfill 2 reached
125k splats at 1302 ms rather than 2254 ms. Both constrained arms settled at
126,617 splats. Main WASM capacity was 99-106 MiB in every arm.

The default is now `Some(0)`. It reached full detail fastest under every
condition and matches the reference default. The cost is progressive feedback:
the coarse level stays until the target arrives. Underfill 2 remains available
when intermediate detail matters more than time to full detail. Throttling is
CDP emulation on localhost, not a WAN replay.

## Pre-warmed codec workers

Workers now boot the codec at module evaluation. The production pool keeps one
idle spare booting during the active job, still one job per worker.

| Condition | Arm | Settle (ms) | Dispatch-to-handler sum (ms) | Worker round-trip sum (ms) |
| --- | --- | ---: | ---: | ---: |
| Localhost | underfill 0 | 1001 | 195 | 387 |
| Localhost | underfill 0 + prewarm | 667 | 58 | 199 |
| 50 Mbps, 20 ms | underfill 0 | 2254 | 147 | 419 |
| 50 Mbps, 20 ms | underfill 0 + prewarm | 2204 | 30 | 282 |

The throttled settle difference is within noise; that condition is network-bound.
Single-worker WASM capacity stayed about 7 MiB. The idle spare adds one booted
worker for the page lifetime.

## End to end against main

Branch head `bc2fdad` (underfill 0 default, worker boot at manifest parse,
prewarmed spare) against `main` at `8154029`, in one plan with two repeats of
three rounds:

| Condition | Arm | First load-ready (ms) | Settle (ms) | 31k ready (ms) | 125k ready (ms) | Decode jobs | Cache peak (MiB) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Localhost | main | 220 | 1879 | 973 | 1260 | 7 | 28 |
| Localhost | branch | 182 | 576 | 576 | 576 | 3 | 21 |
| 50 Mbps, 20 ms | main | 460 | 5325 | 1615 | 2466 | 7 | 28 |
| 50 Mbps, 20 ms | branch | 333 | 2084 | 2027 | 2027 | 3 | 21 |
| Constrained | main | 225 | 1206 | 940 | 1206 | 5 | 7 |
| Constrained | branch | 189 | 497 | 492 | 492 | 2 | 5 |

Settling is 59-69% faster. One metric regresses: under throttling the branch
reaches 31k splats about 0.4 s later, because it skips the intermediate levels.
Main WASM capacity is about the same (100-106 MiB).

One branch run in this plan failed `perf_scenes` validation. A prewarmed worker
received its message within the cross-context clock skew, so
`dispatchToHandlerMs` was dropped as negative for one round. Intervals within
1 ms of zero now record zero; larger negative intervals are still rejected.

## Rejected: queuing the target with initial coarse coverage

This experiment queued each leaf's target in the initial pass, behind the coarse
files, instead of after coarse was ready. Both plans ran two repeats of three
rounds:

| Condition | Arm | First load-ready (ms) | Settle (ms) | Dispatch-to-handler (ms) |
| --- | --- | ---: | ---: | ---: |
| Localhost | after coarse | 222 | 585 | 22 |
| Localhost | concurrent | 373 | 644 | 49 |
| Localhost | concurrent + cold worker boot | 299 | 475 | 3 |
| 50 Mbps, 20 ms | after coarse | 460 | 2222 | 44 |
| 50 Mbps, 20 ms | concurrent | 1719 | 1903 | 1077 |
| 50 Mbps, 20 ms | concurrent + cold worker boot | 1701 | 1905 | 1 |

Throttled first draw tripled. Booting the worker at manifest parse removed the
worker startup delay, but did not recover first draw: the coarse source's own
files share the link with the 8.3 MiB target. The change was reverted. The cold
worker boot was kept; it costs nothing on the critical path. A future attempt
should start the target only after the coarse transfer completes, or deprioritize
it. Browsers expose no fetch priority for this.

## Limits

- Desktop only. This is not physical mobile.
- One hierarchy, whose two leaves share files. Leaves with different targets are
  not covered.
- The endpoints are encoded draw and ready-splat observations, not GPU
  completion or presentation.
- No SuperSplat runtime replay was run. The reference comparison here is source
  behavior only.
- Session drift is visible: the same underfill 0 build settled at 777 ms and
  1001 ms on localhost in two plans. Compare arms only within one plan.
