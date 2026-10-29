# Splat performance TODO

Follow-up work from the [renderer audit](../docs/benchmark-methodology.md).
P0 establishes trustworthy comparisons; P1 investigates measured costs and stress
behavior; P2 turns successful experiments into supported configurations.

## Existing capabilities and scope

Streamed SOG hierarchies already support chunk loading, packed sources and LOD
refinement. Packed/GPU-resident rendering, visibility compaction, projection-cache
and page-residency paths also exist; some are optional or experimental.

Opening a standalone local PLY or bundled SOG follows a separate whole-file path:
read the complete input, copy browser bytes into WASM, decode expanded attributes,
then publish the cloud. Investigate that path independently of streamed SOG.
The audit has not established the cause of the reported SuperSplat loading gap.
An unchecked item below identifies missing evidence or follow-up work; it does
not imply that the corresponding runtime behavior is absent or broken.

Coverage audit: automatic Bencher reporting covers five CPU and 12 pipeline
microbenchmarks. Native completed-frame median/P95/P99 and optional radix GPU
timestamps now have initial real-scene Bencher history (40 metrics each for
Apartment and JunkShop); continuous hardware GPU reporting is not set up.
The manual hardware workflow requires a provisioned trusted runner. Automatic
native bounds-fitting is not a calibrated interior camera or visual approval.
Browser replay reports cadence, loading milestones and partial CPU/cache traces,
not GPU completion, presentation latency or worker CPU time. The active renderer
has no stochastic transparency path; `SortMode::None` is not an equivalent mode.

## P0: Representative assets and fair comparisons

- [x] Prepare complete pinned Apartment, JunkShop SH3 and seven-source Playbot
  inputs in an external cache; reverify their bytes before/after benchmark runs.
  Preserve conflicting/unverified license evidence; no automatic asset fetching
  or model binaries in Git.
- [ ] Calibrate and review scene-specific cameras, orientation and coverage.
  Native automatic bounds-fitting can frame outliers or exterior surfaces and
  is not enough for representative interior/overdraw diagnosis. Keep its initial
  timing history distinct from future calibrated workloads.
- [ ] Establish a reproducible public corpus: a representative slow asset,
  million-splat full files, a multi-source/multi-level SOG hierarchy, and an
  overdraw-heavy scene. Record licenses, source URLs, byte counts and all child
  hashes; keep large generated/downloaded binaries outside Git.
- [ ] Replay the same scene in Dimensify and SuperSplat first, then the other
  audited implementations. Pin revisions and record unsupported configurations
  instead of silently substituting a different rendering mode.
- [ ] Match physical resolution/DPR, camera poses, SH degree, alpha/footprint/blur,
  frustum margins, sorting mode/precision, color handling and temporal settings.
  Review captures and define image tolerances before comparing timings.
- [ ] Repeat independent clean optimized runs, rotate renderer order and keep
  other builds/benchmarks idle. Record hardware, driver, browser, build hash and
  instrumentation mode; distinguish fresh-process, HTTP-cache and live-source
  cache behavior. Do not claim OS/driver caches were flushed without evidence.
- [ ] Define consistent loading milestones across adapters: submission, input
  available, first coarse coverage, first completed frame and target-quality
  settling. Existing readiness/encoded-draw/RAF observations retain their names;
  add a supported completed-frame or visible-pixel protocol before claiming it.

## P0: Complete useful regression history

- [x] Run the new pipeline's initial
  [three-report bootstrap](https://github.com/soraxas/dimensify/actions/runs/40691518690).
- [ ] Seed missing hosted testbeds and verify a normal comparison for all 12
  pipeline metrics. The [following run](https://github.com/soraxas/dimensify/actions/runs/40692688818)
  failed for a missing baseline; bootstrap success alone is not gate verification.
- [ ] Exercise a deliberate pipeline regression and recovery on a disposable PR;
  verify reports, links and authoritative failure before making new checks required.
- [ ] Configure repeatable GPU hardware runs and authenticated Bencher reporting.
  Isolate hardware/driver/browser/settings/instrumentation histories and verify a
  real regression/recovery. CPU hosted runners do not provide GPU coverage.
- [ ] Retain raw samples, captures and environment/asset identities with each
  report. Preserve existing workload semantics; version changed workloads and
  fixture identities. Keep percentile metrics separate from confidence bounds.

## P0: Render correctness and visual regression CI

See the [Bevy / Pixel Eagle audit](visual-regression-ci-audit.md) for source
references, existing coverage, capture contracts and the proposed CI matrix.
Implementation evidence is tracked in [visual regression implementation](visual-regression-implementation.md).
The hosted Vulkan suite passes 22/22 GPU cases. Native Vulkan/Metal/DX12 and
browser WebGPU analytic captures and independent Pixel Eagle comparisons pass.
Reviewed references live in Pixel Eagle; rendered images remain CI artifacts.

- [x] Prove a software Vulkan CI lane and explicitly run selected existing
  ignored raster/packed/buffer tests serially. Record the actual adapter and
  driver, require all captures, and keep hardware-performance eligibility separate.
  Inventory fixture requirements first; the real-leaf bounds reproduction needs
  an external `SOG_REPLAY_DIR` and must not enter an unqualified `--ignored` run.
- [x] Extend existing offscreen helpers with named analytic Gaussian cases and
  expected-image assertions for covariance/falloff, alpha/depth order, SH/color,
  projection and supported storage/render-path parity. Preserve existing LOD,
  compositing, camera isolation, residency and teardown pixel assertions.
- [x] Define an exact capture manifest, bounded readiness/completion, immutable
  capture identities. Keep region/semantic assertions in Git and reference images
  and image diffs in Pixel Eagle; retain temporary capture/result artifacts. Verify
  deliberate blank, dropped-cloud, wrong-order and missing-capture failures/recovery.
- [x] Execute the packaged WASM renderer in CI and assert canvas pixels after
  real readiness. Existing GPU-free gallery navigation checks remain useful but
  do not cover rendering. Establish supported software WebGPU execution first.
- [x] Add Pixel Eagle reporting after analytic assertions work on forks.
  Separate rendering from token-bearing reporting, validate artifact provenance
  and service responses, pin reviewed references and make baseline updates
  deliberate. Missing tokens/references/service errors cannot count as a passed
  visual comparison. Check current free-tier comparison quotas before expansion.
- [x] Add native Metal/DX12 image coverage and Pixel Eagle references.
- [ ] Expand physical Android/iOS image coverage, recording
  backend/device/driver/browser/resolution identities. Evaluate BrowserStack OSS
  eligibility and actual device WebGPU support; preserve controlled hardware
  performance runs and the mobile responsiveness work below.
- [ ] Extend headless injected-input/system replays for camera/LOD/cancellation
  ordering and add narrow supported-feature/platform compile checks where absent.
  Preserve standalone export and headless dependency isolation; expand the matrix
  according to failures and duration rather than copying all upstream jobs.

## P0: Physical mobile comparison and responsiveness

Current capture is desktop-only. The [browser harness](../scripts/perf-browser.cjs)
launches Chromium with a 1000x760 viewport and DPR 1. It records observed DPR,
canvas dimensions and user agent, but does not attach to a physical phone or
establish mobile performance. Native offscreen results also do not represent a
phone browser. The reported Dimensify/SuperSplat mobile difference is a P0 repro.

Current WebGPU/WASM defaults request up to 4 million splats, a 256 MiB CPU source
cache, 256 MiB GPU source residency and 512 MiB selection budget per scene, before
device/admission limits. These are policy ceilings, not simultaneous allocations
or detected available memory. Defaults distinguish WASM/native and WebGL/WebGPU;
they do not establish a measured phone-specific quality or frame-time policy.
Sources: [LOD settings](../crates/splat/src/streaming/settings.rs),
[GPU budgets](../crates/splat/src/renderer/gpu.rs) and
[device limits](../crates/splat/src/renderer/limits.rs).

- [ ] Reproduce the reported difference on the same physical phone/tablet,
  browser and asset. Identify the exact SuperSplat URL/build (editor versus
  published viewer), backend and effective quality; separate slow startup,
  stuttering gestures and sustained low frame rate. Record cold/warm captures.
- [ ] Add physical Android and iOS/iPadOS capture paths as supported by each
  browser: remote attachment or in-page export/manual replay. Desktop launch,
  Chromium CDP inventory and emulated touch must not be prerequisites or proof of
  a physical mobile run. Keep runner-host identity separate from target-device
  identity; do not infer hardware from a redacted adapter.
- [ ] Parameterize CSS viewport, actual device DPR, orientation and rendering
  resolution. Test both equal physical pixel counts across renderers and each
  viewer's actual shipping defaults. Record CSS size, canvas backing size and
  internal render targets independently; do not override a phone's DPR to 1 and
  label the result representative.
- [ ] Isolate pixel cost with render-scale sweeps and overdraw-heavy/near-side
  views, then splat/LOD budgets and SH/sort settings at fixed resolution. DPR 3
  produces nine times the pixels of DPR 1 at equal CSS size. Check actual
  SuperSplat settings too: its audited source also uses devicePixelRatio, so DPR
  alone is not an established explanation for the difference.
- [ ] Measure CPU/source/selection budget sweeps, decoded/transfer/staging peaks,
  GPU bandwidth pressure and old/new overlap. Device binding limits are ceilings,
  not a reliable available-memory signal. Include tab reload/device-loss/OOM
  evidence and explicitly unavailable memory metrics.
- [ ] Measure main-thread scheduling, worker decode, touch handling, UI updates
  and GPU stages separately. Include gesture-to-render response and frame tails
  while LOD work is pending; preserve the distinction between encoded cadence,
  GPU completion and physical presentation latency.
- [ ] Run sustained motion/refinement sessions as well as short bursts. Record
  refresh rate where known, battery/power mode, charging state, cooling/run order
  and observed degradation; distinguish measured throttling from a hypothesis.
  Do not assume every mobile browser exposes thermal or memory instrumentation.
- [ ] Use measured results to design bounded mobile quality: rendering scale,
  target pixel error, splat/cache/selection budgets, decode concurrency and
  per-frame upload work. Evaluate frame-time adaptation with hysteresis and user
  overrides, preserving coverage and preventing quality/residency oscillation.
- [ ] Compare existing packed/compaction/projection-cache/paging configurations
  and supported WebGL fallbacks on physical devices before choosing defaults.
  Tag mobile Bencher histories by device, OS/browser, backend, resolution and
  quality; retain unavailable adapter identity rather than granting desktop
  publication eligibility. Require a repeatable phone regression/recovery.

## P1: Loading latency, copies and responsiveness

- [ ] Trace both streamed SOG and standalone file loading through read/network,
  ZIP/header parsing, WebP/PLY decode, validation/bounds, allocation/copies,
  composition, upload, shader/pipeline initialization and first completed draw.
  Separate startup costs from steady-state work and queue delay from execution.
- [ ] Add worker-side decode/transfer timings and main-thread long-task/stall
  observations. Existing asynchronous wall intervals are not worker CPU time.
- [ ] Measure peak JS input, WASM backing, decoded attributes and transfer copies
  for local PLY/bundled SOG. Compare with the packed streamed path using matched
  content; do not infer a cross-renderer result from different synthetic formats.
- [ ] If profiling identifies the generic PLY parser, expanded conversion or
  copies as dominant, evaluate typed binary parsing, worker execution, reduced
  copying and progressive publication where the format permits it. Preserve
  attribute validation, bounds, cancellation and platform allocation limits.
- [ ] Measure coarse-first latency and refinement stalls on real SOG hierarchies;
  investigate source granularity, prefetch and overlap of fetch/decode/upload
  only where measurements justify a change.
- [ ] Surface an actionable source-admission reason before an apparent loading
  stall. Test the known 4 MiB worst-case codec reservation allowance; investigate
  safe tightening after metadata inspection without weakening allocation bounds.

## P1: GPU stages, sorting and memory

- [ ] Extend supported GPU timestamps to projection, compaction, sorting,
  rasterization and compositing; measure upload/copy completion where available.
  Report missing capabilities explicitly. Preserve originating frame IDs and
  account for overlapping passes instead of summing them as total GPU time.
- [ ] Record sort frequency, sort age relative to the displayed camera, dirty
  invalidations and any depth readback/worker queue delay. Compare static,
  rotation, translation and LOD-change cases separately.
- [ ] Benchmark supported sort implementations and 16/24/32-bit radix precision
  at matched image quality. The current native performance runner fixes Radix
  and 32-bit depth; isolated CPU sorting does not measure production GPU sorting.
- [ ] Treat stochastic transparency as a separate renderer experiment, not a
  sorting toggle or an existing benchmark. If implemented, compare matched
  resolution/quality with sorted alpha, including overdraw, temporal noise,
  camera-motion artifacts, convergence time and median/P95/P99 frame cost.
  Use reproducible seeds and repeated temporal captures; a single PNG cannot
  establish stochastic image quality or justify removing depth ordering.
- [ ] Observe projected/surviving/drawn splats separately from selected counts
  and allocated capacity. Quantify overdraw sensitivity using near-side views.
- [ ] Combine existing source/selection/sort accounting with missing components:
  projection planes, texture/codebook sharing, upload staging, CPU/WASM backing,
  pending decode and old/new generations. Deduplicate shared allocations and
  record transient peaks; label partial accounting instead of calling it total
  VRAM or process RSS. Use external memory measurements where supported.
- [ ] Compare existing packed, GPU-resident and snapshot paths, frustum policy,
  compaction, projection caching, radix precision and experimental paging at
  matched quality. Measure CPU, completed-frame tails, memory and startup cost
  before changing defaults. Preserve explicit fallbacks and backend identity.

## P1: End-to-end LOD and residency stress

- [ ] Run sustained constrained-budget turn-away/return and eviction/revisit
  replays on multi-source hierarchies; verify recovery and absence of unnecessary
  re-decode/re-upload for retained sources.
- [ ] Teleport or rapidly reverse the camera while fetch/decode/upload is pending;
  measure time to restore useful coverage, obsolete work and visible hitches.
- [ ] Cancel, unload or replace a scene during each pipeline stage. Verify late
  completions cannot revive it and pinned physical resources remain owned until
  outstanding work releases them.
- [ ] Change CPU/source/selection budgets and concurrency mid-flight; test
  impossible budgets and temporary old/new overlap without holes or churn loops.
- [ ] Stress multiple scenes and cameras sharing sources. Validate global versus
  per-scene budgets, per-view sorting, sharing/deduplication and independent unload.
- [ ] Inject network latency, bandwidth limits and failed/retried resources;
  distinguish HTTP caching from live residency and verify stable recovery.
- [ ] Record coverage/quality, convergence, median/P95/P99 completed-frame times,
  long stalls, queue/in-flight/pinned bytes, evictions and wasted fetch/decode/upload
  work. A visually covered, stable target selection is required; empty queues alone
  do not establish completion. Make truncated telemetry explicit.
- [ ] Evaluate upload bytes/work per frame and backpressure under pressure; tune
  cancellation, prioritization or scheduling only against reproducible hitches.

## P2: Supported configurations and release criteria

- [ ] Expand the P0 physical-mobile comparison across representative device
  generations and constrained-memory systems, plus desktop integrated/discrete
  GPUs. Define the supported device/quality matrix from measured results.
- [ ] Establish measured quality/performance configurations and promotion criteria
  for optional projection caching, compaction and paging. Document device limits,
  memory tradeoffs and fallback behavior; retain a dependable default path.
- [ ] Publish a comparison with raw artifacts, matching-quality captures, repeated
  runs and explicit unsupported metrics. Separate measured bottlenecks, confirmed
  improvements and remaining hypotheses.

Start with the reported physical-phone comparison and real-asset loading trace.
Select implementation work from the resulting bottleneck rather than copying
every reference optimization.

## Performance control repeatability

- [ ] Quantify repeated same-commit variance on each hosted runner class. Record
  CPU identity, runner image, toolchain and raw reports; CPU model alone does
  not capture scheduling or frequency variation.
- [ ] Compare base and head on the same controlled worker, or use fixed hardware
  for performance decisions. Establish an explicit unstable-run verdict from
  measurements rather than repeatedly submitting until a check turns green.
- [ ] Investigate current main pipeline-compose and PR sort/decode alerts. The
  visual change does not modify their benchmark or algorithm sources; the
  cause remains unproven. Keep alerts, branch protection and thresholds intact;
  never promote PR data merely to clear a regression.
