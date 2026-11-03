# Browser camera checkpoint validation (2026-11-03)

The browser harness previously saved only case endpoints. Opt-in checkpoint
recording now captures intermediate drag poses, checks fresh telemetry against an
explicit reference and rejects drift before emitting BMF. The protocol and
reference digest separate these measurements from unchecked replay histories.
See the [replay guide](browser-replay.md#camera-checkpoint-checks).

## Local validation

Harness base: main `0bc6f0d`, with this branch's script changes. Renderer package:
clean `ca57bd694b1271829417fcc31d9b58ee82308a82`, optimized instrumented WASM SHA-256
`615cf8ce9358de09ad3f63811bcfc6ea2087b52d285ecc73354030b4393b2fd1`.
Every packaged file was verified against its build manifest. There are no renderer,
worker or host-page source differences between that package revision and the
harness base; no renderer rebuild was needed for these script changes.

Use committed `tiny.ply`, Chromium 153.0.8010.12, a 1000 x 760 backing canvas at
DPR 1, local serving, one process per run and 500 ms requested action duration.
The active WebGPU adapter is privacy-redacted; browser inventory reports Apple
M5 Max / Metal. These are functional checks, not hardware performance evidence.

- Full recording passes: seven cases and 29 checkpoints in each cold/warm mode.
- Initial independent motion replay rejects `cold/moving-main/return-15` despite
  matching earlier checkpoints. The failing run remains in raw data and no BMF
  file is emitted.
- Checkpoint mode adds telemetry barriers after pointer reposition and release.
  The viewer sums motion per update, so batching those transitions with a later
  button press is a timing dependency worth avoiding. Barriers alone do not make
  the camera path deterministic: a subsequent isolated replay still rejects
  `warm/moving-main/return-15`. Its retained `failedCameraRun` is excluded from
  timing aggregation. For a separate offline check, that trace was placed in `runs` with the final-schema
  reference and digest; it still failed at the same checkpoint. Normal saved-report
  validation does not inspect `failedCameraRun`.
- An independent static cold/warm reference check passes with the final
  validator. Saved-raw reconstruction passes and rebuilds 34 cadence/readiness
  metrics; changing a saved camera position then causes reconstruction to fail.

Some preliminary functional checks and script tests overlapped. The isolated
motion repeat ran without another browser check or build. No timing delta or
renderer optimization is inferred from these runs, and none was uploaded to Bencher.

The full `just ci-scripts` recipe passes: 161 Python tests, 51 browser/helper
tests (one optional live worker-profiler test skipped), 19 worker tests and
32 visual/helper tests, plus catalog and syntax checks. Python formatting and
Git diff checks pass. No Rust behavior changed or renderer/GPU suites reran.

Code review found stale samples outside a case's time window and loss of partial
traces on telemetry failure. Checks now require ordered, in-window observations;
an active camera run retains completed cases and the partial case on failure.
The review recheck found no remaining material findings. Partial-timeout retention
was checked by inspection; the real failure tests exercised completed-run drift.

Raw reports, explicit references, captures, validation results and package manifests
remain in ignored `dist/camera-replay/` and `dist/viewer-settings/`. No images or
generated camera references enter Git. Fixed Pixel Eagle images and Bencher
thresholds are unchanged.

## Remaining gap

Moving-view performance is still diagnostic. A matched set of sampled checkpoints
would not prove an identical pose at every rendered frame. Establish repeatable
real-scene trajectories, then compare clean baseline/candidate builds at the same
physical resolution, settings and hardware. Do not relax bounds to admit the
current drift or compare checkpoint-mode timings against older uninstrumented
gesture histories.
