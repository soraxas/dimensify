# Comprehensive splat benchmarks

Goal: measure production loader and streaming policy costs, plus real viewer
loading and frame latency. Extend the existing Criterion, Bencher and browser
replay paths; never label CPU policy or requestAnimationFrame as GPU time.

## Design and acceptance

- Preserve the five existing CPU workload identities and required checks.
- Add an optional `pipeline` feature to `dimensify_perf`; its default dependency
  graph remains headless. Production streaming internals are exercised through
  the existing opt-in `test-support` module, without copying policy algorithms.
- A separate pipeline CI workflow measures native CPU operations without a GPU,
  uploads all metrics to Bencher, and requires explicit main baseline bootstrap.
  It is not a required branch check until its baselines are verified.
- Loader inputs are deterministic and generated before measurement. Include
  binary PLY, a real SOG ZIP bundle, packed SOG and expanded composition.
- Policy workloads include spatially varied leaves, perspective views, a
  recorded pressure boundary, cache admission and resident scheduler updates.
  Verify counts, coverage, budgets and actual state changes outside timing.
- Opt-in viewer telemetry supports real browser replay. Capture loading,
  refinement/settling, cold/warm source behavior, frame intervals and bounded
  CPU-stage counters. GPU timing is separately optional and explicitly marked
  unavailable when unsupported. Readiness, frame cadence and visible pixels
  must have distinct names and semantics.
- Browser runs record asset identity, build profile, exact commit, browser,
  viewport, adapter and settings; reject software GPU performance publication.
  Raw observations and Bencher Metric Format are saved for review. No automatic
  browser publication from heterogeneous hosted hardware.
- Synthetic fixtures establish reproducibility, not SuperSplat parity. Allow a
  supplied scene/camera replay for diagnosing the user's slow asset later.

## Tasks

- [x] Loader fixtures and Criterion workloads (`tools/perf/src/pipeline.rs`,
  `tools/perf/benches/pipeline/loading.rs`, optional Cargo dependencies).
  Export `workloads(&mut Criterion)`; names: `pipeline-ply-v1/10000`,
  `/100000`, `pipeline-sog-bundle-v1/65536`,
  `pipeline-sog-packed-v1/65536`, `pipeline-compose-v1/65536`.
- [x] Production policy harness in
  `crates/splat/src/streaming/runtime/test_support/benchmarks.rs`, entry point
  `tools/perf/benches/pipeline.rs`: manifest parse 1k/10k, selection 1k/10k,
  pressure relocation, cache admission and resident scheduler replay.
- [x] Browser telemetry and replay, tests and raw/BMF result artifacts. Keep
  this independent of loader fixtures and Bencher credentials.
- [x] Native offscreen completed-frame runner with acknowledged production draws,
  optional radix timestamps, validated captures and clean-build publication guards.
- [x] Suite-specific complete-result validation, isolated testbeds, separate
  pipeline CI, Just recipes, documentation and migration checklist.
- [x] Verify fixture tests, full release Criterion suite, Python/JS contracts,
  native/WASM compilation and a real browser replay. Review the combined diff.

Validation so far: 22 Rust fixture/policy/helper tests, 22 Python tests and 19
JavaScript contracts pass. Strict pipeline Clippy, formatting, workflow linting,
WASM instrumented viewer check and isolated export pass. Actual native GPU smoke
and cold/warm PLY plus limited-LOD browser replays pass. Independent review found
stale output, truncated RAF histories, unknown-adapter publication and degenerate
motion-period risks; each now has a regression check. The full release Criterion
run completed all 12 finite metrics in `dist/performance-pipeline-expanded-v3`.
That dirty local run overlapped other verification and is acceptance evidence,
not a publication baseline. Hosted pipeline baseline bootstrap remains pending.

## Execution notes

The user requested implementation after the coverage audit. Work in the existing
clean checkout; the CPU and browser tasks own separate files. Parallel work uses
the dispatching-parallel-agents skill. Preserve unrelated work and do not publish
synthetic results as a main baseline.
