# Compaction and projection-cache evidence (2026-11-18)

Apple M5 Max, headless Chromium (WebGPU, ANGLE Metal), 1000x760 at DPR 1,
streamed Playbot fixture `3a40bceb8ffa`, Packed backend. One `just perf-ab`
plan, two repeats of three rounds, alternating arm order. All arms are the
same candidate build (on `ed28703`) with automatic detail off
(`--lod-auto off`), differing only in `--compaction` and `--projection-cache`.
Telemetry confirmed each arm's flags, and the projection arms allocated 32 MB of
projection buffers, so the cache really ran.

| Condition | Arm | Load-ready (ms) | Settle (ms) | Motion p50 (ms) | Motion p95 (ms) | Cache (MiB) |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| local | default | 206 | 600 | 15 | 29 | 21 |
| local | compaction | 191 | 571 | 15 | 29 | 21 |
| local | projection | 193 | 602 | 16 | 31 | 21 |
| local | both | 197 | 617 | 16 | 31 | 21 |
| constrained | default | 195 | 514 | 15 | 28 | 5 |
| constrained | compaction | 202 | 536 | 16 | 29 | 9 |
| constrained | projection | 196 | 542 | 16 | 31 | 5 |
| constrained | both | 198 | 545 | 16 | 30 | 9 |

Neither feature improved anything beyond run-to-run spread. The projection cache
added 1-3 ms to motion P95 and 32 MB of GPU buffers. Both remain off by
default. Frame intervals here are vsync-bound (raster is about 1.2 ms for 500k
splats, see [adaptive transparency evidence](adaptive-transparency-evidence.md)),
so a GPU saving below the refresh interval cannot show on this machine.

## Limits

- Desktop only. Both features target GPU-bound devices: compaction draws fewer
  off-screen splats, and the projection cache projects each splat once per frame
  into a 32-byte record that radix sort and the vertex shader share, instead of
  the vertex shader projecting it once per quad corner. It does not reuse
  projections across frames. On desktop that math is cheap and the extra pass
  and record traffic cost more. A phone run is still needed before ruling them
  out.
- No GPU timestamps in this run; a native `just perf-lod` comparison with radix
  and projection timestamps would isolate the pass costs.
- The projection cache still applies only on **Apply and reload scene**; making
  it switch live needs a stream resubmission hook.

## Radix sort reuse

GPU radix sorting used to run every frame, even when nothing had changed. The
sort now reuses the previous order and visible count when the view, projection,
viewport, cloud transform, count, time, depth bits, compaction, projection cache
and data identity are all unchanged for two consecutive frames. It still
re-sorts at least once every 60 frames as a safety net. Streams invalidate reuse
while they have pending work, generations or downloads, and in-place GPU
publications (progressive PLY appends, reused resident slots) invalidate their
cloud directly. Views timed on a device with timestamp queries always sort.

Native runner, Apple M5 Max / Metal, Apartment (661,466 splats) with the interior
camera, 1280x720, bench profile, base `9e188f9` versus the uncommitted candidate,
five alternating rounds each:

| Case | Base completed median (ms) | Candidate completed median (ms) | Candidate sorts dispatched / reused |
| --- | ---: | ---: | --- |
| main static | 5.99 | 5.36 | 8 / 472 |
| side static | 6.36 | 4.35 | 8 / 472 |
| main moving | 5.88 | 5.80 | 480 / 0 |
| side moving | 7.20 | 5.28 | 480 / 0 |

The counts confirm the skip. The frame times do not resolve the saving: the
machine had a load average of 9-19 from other processes, per-round medians for
one case ranged from 4.5 to 8.4 ms, and side moving improved by 1.9 ms although
it sorts every frame. An earlier clean run measured the radix and compaction
pass at 0.91 ms per still frame on this scene
([real-scene evidence](real-scene-benchmark-evidence.md)), the expected saving on
this GPU. The publication gate rejected these runs as uncommitted, so they are
local diagnostics only (`dist/ab/sortskip`).

GPU coverage: `still_frames_reuse_the_radix_order_until_the_cloud_changes`
checks reuse, identical redraws and re-sorting after in-place data changes, with
and without compaction. The existing progressive PLY GPU test caught the first
version reusing an order across in-place appends.

