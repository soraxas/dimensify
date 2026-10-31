# Viewer settings review follow-up (2026-10-31)

- [x] Verify the new review against current code. Keep the viewer's WebGPU
  defaults: its feature graph enables WebGPU, so a WebGL-only viewer branch
  would be unreachable. Gate the backend-override quality test in actual
  library WebGL-only builds instead.
- [x] Reproduce malformed Bencher nested fields, untrusted metric/name output
  and redundant disabled-marker inserts with failing regression tests. Validate
  nested response types, print only known names and validated numeric fields,
  and update control markers only when their state changes.
- [x] Qualify host compaction and staged Reset behavior, describe mixed-backend
  allocation scope without claiming differing SH layouts, and include the
  unresolved CPU alert in the viewer evidence record.
- [x] Strengthen touch acceptance with observed panel bounds and an identical
  canvas gesture that must change the camera; expose geometry only for perf UI
  observations, without adding viewer action hooks.
- [x] Pass 259 regular splat tests, five native quality tests, three pure-WebGL
  quality tests, native/WASM strict Clippy, 158 Python tests, 32 JS helper tests
  and formatting. The 45 ignored GPU/external cases were not rerun here.
- [x] Build clean optimized WASM on `ca57bd6` and pass Chromium acceptance,
  including measured panel touch with no camera movement and the same canvas
  gesture with required movement. Use the observed Close control at narrow
  width, where the panel overlaps the toolbar. Record revision/hash and limits
  in the evidence document; keep captures in ignored artifacts.
- [x] Verify `38b0a6f` hosted CI: gallery cache export, native/WASM visual checks
  and CPU/pipeline gates pass. Cache admission is 14.194 us versus base samples
  14.777/14.103/14.328 us, with no alert. Preserve the prior 123.82 us failure:
  the later pass does not explain it or establish an optimization.

# Viewer settings CI failure investigation (2026-10-31)

- [x] Inspect PR #21 revision `0fc8fdb` logs and saved Bencher reports. Gallery
  build and browser acceptance passed, then isolated compiler-object export ran
  out of disk (4.08 GB free before export). Enable the pinned action's supported
  post-job target cleanup after all build/artifact consumers finish.
- [x] Reproduce the uploader's generic-error masking with failing tests. Validate
  JSON reports even when the CLI returns nonzero; show allowlisted regression
  metrics and a validated report link, distinguish missing baselines and invalid
  responses, and never print service stderr. Replay the actual saved alert.
- [x] Pass all 155 Python tests, including the new failure-path regressions,
  plus actionlint and offline zizmor. No Rust/render behavior changes in this
  follow-up; keep local validation distinct from the next hosted run.
- [x] Verify post-job cache export in hosted gallery CI on `38b0a6f`, run
  `41199698738`. The full job passes after enabling target cleanup.
- [ ] Investigate the genuine Bencher cache-admission alert: 123.82 us versus a
  14.22 us same-runner base mean (17.064 us upper limit). Candidate confidence
  interval is 68.964-177.42 us; the three base means are 14.181/14.466/14.013 us.
  Benchmark, replay and cache sources match main, but that does not establish
  a cause or invalidate the alert. Preserve failed reports and thresholds.

# Viewer settings merge with runtime module split (2026-10-31)

- [x] Preserve main's streaming runtime module split and local codec/sort crate
  locations. Carry per-scene renderer limits, source storage and page selection
  into the extracted `systems.rs`; do not restore the old monolithic module.
- [x] Validate the combined branch: 258 regular splat tests, five quality tests,
  native/WASM strict Clippy, formatting, 150 Python tests and 32 visual/helper
  tests pass. The 45 ignored GPU/external cases were not rerun. Preserve draft
  status and let CI verify the pushed merge revision separately.

# Whole-file reload guard and validation correction (2026-10-31)

- [x] Return PR #21 to draft. Prior local checks covered pending streams but
  missed Apply during whole-file decoding; passing suites did not establish that
  load ownership was safe for every source type.
- [x] Reproduce cancellation with the production file request system and the
  committed tiny PLY fixture. Include queued whole-file commits in the shared
  load guard and prevent renderer reloads from cancelling a file selection.
- [x] Remove unreachable WebGL-only viewer branches: the viewer feature selects
  WebGPU. Keep actual library WebGL fallback coverage. Derive mixed-layout
  diagnostic limits only when enabled, reusing each scene's computed limits.
- [x] Pass 258 regular splat tests and five quality tests, native/WASM strict
  Clippy and formatting. Build native/optimized WASM from clean `6b43865` and
  pass real-input browser acceptance against that package. Record the revision,
  hash and reproduction commands in the validation evidence. Keep PR #21 draft;
  this local verification does not claim that new-revision CI has completed.

# Viewer settings PR review (2026-10-31)

- [x] Reproduce Apply during initial/replacement loads and differing packed
  projection layouts with failing regression tests. Preserve new scene requests
  and reject reloads while another candidate is pending; ordinary commits keep
  live compaction and do not promote staged storage choices.
- [x] Bound wasm32 CPU cache bytes to a representable whole MiB, normalize the
  defensive WebGL-only viewer defaults, protect storage fields behind construction
  methods, and publish per-scene selection limits during mixed-layout reloads.
- [x] Strengthen toolbar/global-default tests, show the disabled render action,
  preserve the original browser failure when diagnostics fail, honor the custom
  Playwright module, and attribute GPU versus reload checks to their actual suites.
- [x] Pass 254 regular splat tests, five quality tests, the focused pure-WebGL
  renderer override test, native/WASM strict Clippy and native/optimized WASM
  viewer builds. Pass 150 Python and 32 visual/harness helper tests. Expanded
  browser acceptance holds an initial load, rejects Apply and preserves live
  compaction through readiness, then checks reload/rollback/camera/touch behavior.
  Independent follow-up review found no remaining material issues. Keep images
  in artifacts and preserve visual references and performance thresholds.

# Viewer quality and advanced settings (2026-10-30)

- [x] Identify analytic sorted/stochastic/packed fixtures and Apartment, JunkShop,
  Playbot performance scenarios before editing. Preserve renderer defaults.
- [x] Add a native/WASM Feathers settings panel over existing quality, memory,
  sorting and streaming resources, with explicit applicability and session scope.
- [x] Add optional per-stream renderer configuration; preserve global defaults
  for existing consumers and account for source allocations across backends.
  Stage backend/cache reloads separately and retain the old scene/camera until
  renderer-ready replacement; cancel failed/superseded work without promotion.
- [x] Reproduce and fix CPU-mode transition failures: draw the complete CPU
  permutation after compaction, and rearm production CPU sorting for stationary
  replacements, late sorted buffers and changed cloud transforms.
- [x] Pass 26 explicit Metal GPU correctness cases, including the new draw-count
  transition. Browser acceptance exercises quality/reset controls, all three
  backends, projection reloads, retained camera/coverage, failed reload retry,
  failed-file recovery, narrow layout and panel touch ownership.
- [x] Complete native/WASM builds, regular suites and independent follow-up
  review. Publish four clean Playbot browser diagnostic reports (three processes
  each, 860 observations). Static/reset cameras and scene pixels match exactly;
  keep moving-view camera divergence, readiness/tail costs and sampled WASM
  capacity increases explicit rather than claiming a performance improvement.
- [x] Inspect main/PR Pixel Eagle captures without changing approved references:
  browser images match exactly; native one-level differences match an earlier
  main run exactly. Record the pre-existing fixed-reference settings mismatch
  and motion-replay camera drift in the TODO and
  [validation evidence](viewer-settings-evidence.md).

# Native Gaussian benchmark evidence validation (2026-10-30)

- [x] Summarize current loading, LOD, sorting, stochastic-quality and memory gaps
  at the top of the performance TODO, linking detailed acceptance tasks. Record
  the main cache-admission alert, contrasting historical/same-runner evidence,
  and missing safe uploader diagnostics; preserve thresholds and unresolved cause.
- [x] Reproduce acceptance of edited repeat values with an unchanged median,
  missing/altered captures, duplicate frame IDs and contradictory completion/
  sort/draw evidence before fixing the validators. Reuse one capture checker for
  flat and packed LOD run/upload paths, including dimensions, RGBA hash, actual
  nonblank pixels and unsafe paths/symlinks. In-memory aggregation alone is not
  capture verification; publication always supplies the saved report paths.
- [x] Recompute and match saved repeat values/units as well as aggregate metrics.
  Reject radix samples from another originating frame and failed timestamps
  carrying durations; retain unavailable timing as unavailable. The flat
  protocol requires exactly one acknowledged sort/draw, including integer counts.
- [x] Pass the focused scene/LOD checks and full script suites: 150 Python tests,
  JavaScript/worker checks with one existing skip, and CI formatting. No viewer,
  shader, measured workload identity or performance threshold changes.
- [x] Retain the original CPU probe source/manifest/lock byte-for-byte with
  [staging commands](../tools/perf/probes/sog-expansion/README.md). Build it from
  the locked historical codec snapshot and match complete real-fixture output
  hashes for both scenes. Name the actual CI jobs for codec checks.
- [x] Revalidate 12 saved native suites, 36 processes and 144 captures with 1,104
  unchanged metric values. These are historical evidence checks, not new render
  measurements, Pixel Eagle reference approvals or performance-gate passes.
- [x] Complete final review and full script checks after tightening flat counts;
  verify all 54,960 saved frames satisfy the exact rule with unchanged metrics.
  Read-only review found no remaining blocking issues.

# Gaussian timing repeatability investigation (2026-10-30)

- [x] Pull main through merged PR #19; verify its CPU/pipeline gates, builds and
  visual checks passed. Preserve the previous local evidence commit on its branch.
- [x] Run 12 clean main native processes using one exact executable, alternating
  two control groups per scene. Verify all recorded identities and 24 identical
  static captures; publish/revalidate four Bencher reports and 368 metric values.
  One-shot JunkShop decode medians differ 67.0% without a code change; retain that
  variance and do not treat small loading differences as established gains.
- [x] Verify identical historical spike camera sequences and sort/draw counts.
  Localize much of the extra wall interval to app update and completion waiting;
  do not interpret app wall time as pure CPU execution or sum overlapping passes.
  Rerun the exact historical executable diagnostically; the spike does not recur.
  An empty OS stack sample and failed inspection control establish no stack cause.
- [x] Isolate warm native shared expansion with matching source/dependency locks
  and three processes per revision/scene. Retain all samples and full output hashes;
  current medians are 9.5%/3.0% lower for Apartment/JunkShop. Exclude this throwaway
  probe and profiled/smoke captures from normal performance publication.
- [x] Address PR #19 documentation feedback: describe unmeasured browser/mobile
  results correctly and document the maintained codec formatting exception.
- [ ] Capture usable host execution/submission/wait traces under the actual slow
  condition and repeat on an idle fixed GPU worker. The cause remains unresolved;
  preserve adverse reports and thresholds. See [evidence](sog-repeatability-evidence.md).

# Expanded SOG scale-codebook reuse (2026-10-30)

- [x] Replace three exponentials per expanded splat with a fixed 256-entry
  scale lookup shared by native and browser-worker decoding. Preserve exact
  values, padding, unused invalid entries and first-reference error context.
- [x] Add decoder regressions for all byte indices, non-finite/overflowing/
  underflowing and missing entries, zero count and unused entries. Confirm an
  eager-rejection mutation fails the unused-entry test, then restore production.
- [x] Pass 231 regular splat tests and six codec tests, strict codec Clippy and
  native/WASM worker compilation. Preserve the 44 separately scoped ignored
  GPU/external-fixture tests rather than treating them as executed coverage.
- [x] Include the shared codec explicitly in CI formatting, headless tests/lint
  and WASM worker Clippy; dependency compilation does not run its unit tests.
- [x] Compare two batches of three clean optimized Apartment/JunkShop processes
  per revision, including an interleaved repeat, with matched cameras,
  resolution, SH and GPU settings. Verify all 48 static captures exactly and
  publish eight reports / 736 metric observations, including slower frame tails.
  See [measurement evidence](sog-scale-codebook-evidence.md).
- [ ] Resolve mixed frame/startup results with controlled same-binary repeats
  and stage isolation before claiming an end-to-end improvement. Interleaved
  decode medians decrease 7.1%/9.8% with overlapping ranges; this does not establish a
  browser/mobile or rendering speedup or pass a GPU regression gate.

# Gaussian performance coverage audit (2026-10-29)

- [x] Fix PR CPU baseline availability without mixing hosted hardware cohorts:
  measure the exact PR base three times on the candidate's runner, verify clean
  source/provenance and every raw log, and publish a separate per-attempt base
  branch before the candidate. Keep Bencher's three-report requirement, 20%
  threshold, computed-boundary checks and alerts. Apply to both CPU suites;
  preserve explicit main bootstrap and main history. The earlier pipeline
  bootstrap ran on EPYC 9V74, not the missing EPYC 7763. Local regression and
  workflow checks cover the new path; hosted verification follows the PR push.

- [x] Address performance PR review findings: verify complete hierarchy dependencies,
  bind browser bundles by entrypoint, compare native executable identities and
  select loading phases by name. Run the raw LOD JSON schema before aggregation;
  verify capture bytes/hash/nonblank content from saved reports and reject
  generated sources. Keep unavailable GPU timestamp capabilities explicit,
  retain lifetime GPU samples once per browser round, propagate falsy async
  failures, bound replay options and reject incompatible dispatch settings early.
  Strengthen source-allocation, category-accounting and Bencher naming assertions;
  canonicalize UUIDs before rename-plan deduplication. Resolve PATH commands
  while preserving launcher symlinks and virtual-environment behavior. The worker test already ran
  in CI; update the documented manual command to include it.
- [x] Consolidate benchmark process queries and offscreen GPU lifecycle/capture
  helpers in a separate refactor. Preserve workload plugins, timestamp/HDR policy,
  raw schemas and GPU wait/readback semantics. Verify 47 focused Rust tests,
  strict native Clippy, WASM GPU-library compilation and seven byte-identical
  baseline/candidate captures on Metal. Native-only exports keep sysinfo outside
  the browser library. These CI-profile smoke captures establish correctness,
  not an optimization result; see [evidence](perf-shared-support-evidence.md).
- [ ] Establish pipeline CPU history on the EPYC 7763 testbed: the PR #16 gate at
  `249a600` rejected a missing baseline for `pipeline-cache-admission-v1/1024`
  on `pipeline-cpu-v1-77bb82cc5b4c75b1`, with zero Bencher alerts. Preserve the
  three-main-report requirement and regression thresholds; do not treat missing
  history as a measured slowdown or bootstrap unrelated hardware cohorts.

- [x] Close desktop/headless packed LOD measurement gaps using the prepared
  inlier-data Playbot hierarchy: actual projected completed frames, GPU pass
  histories, deduplicated live owner buffers, verified optional Metal allocation
  accounting and separate worker-profile diagnostics. Clean producer `aff3c12`
  measured six native processes, 12 normal browser replays and six profiling
  replays. Publish/revalidate 883 values in five reports and add 11 focused plots
  with 37 readable series; preserve all 22 earlier plot configurations/order.
  Verify 233 regular splat tests, 22 existing GPU cases, one new actual-GPU owner
  retirement case, 21 flat/14 LOD runner tests, Clippy, optimized WASM builds,
  117 Python tests, script/worker suites, real headless profiling and workflow
  checks. Fix review findings for actual projection acknowledgement, extracted
  owner retirement, contradictory profiling flags and unsafe phone coordinates.
  The user chose desktop/headless first: adaptive phone replay and physical
  device results remain TODOs. Exact worker CPU and portable driver residency
  remain unavailable; the verified Metal API counter is not physical VRAM.
  See [evidence](lod-resource-metrics-evidence.md), [measurement boundaries](../docs/performance-resources.md)
  and the [implementation plan](../docs/superpowers/plans/2026-10-30-lod-resource-metrics.md).

- [x] Validate and publish the resource collector cohort: native checkpoint RSS,
  OS peak RSS and CPU accounting, decoded/GPU buffer capacities, browser/main
  and worker WASM capacity, optional JS heap, worker wall-time coverage and
  independently attributed GPU raster/compositing/projection timestamps.
  Preserve raw missing-capability reasons, separate new instrumentation from
  historical workloads, and retain total GPU residency, pure worker CPU and
  physical mobile measurements as explicit gaps. See
  [measurement boundaries](../docs/performance-resources.md).
  Verify 230 regular tests, 22 explicit GPU correctness cases, 21 runner tests,
  native Clippy, script/worker checks and production WASM builds. Run three native
  processes covering all native workloads and 12 browser replays; preserve all 12 static capture hashes and
  full eligible raster/compositing coverage. Publish and revalidate 590 metrics
  in five reports. Add nine focused resource plots while preserving all 13
  earlier plot identities/order; record [evidence](resource-metrics-evidence.md).
  Fix service-normalized repeated-dash aliases after readable renames with a
  failing-first collision-rejection regression and 57 passing performance tests.
- [x] Exercise compact projection timestamps and actual allocation lifetimes in
  a packed/streaming native LOD replay; flat-cloud fixtures do not cover that path.
  Record browser LOD GPU timestamps and tracked allocation observations separately
  from configured budgets, source-cache payload and WASM capacity. Retain
  physical mobile and total driver residency as separate measurement gaps.

- [x] Remove redundant expanded SOG plane allocations/copies using checked
  ownership transfer, preserving a copy fallback for incompatible capacities.
  Verify exact attribute bytes and malformed-plane rejection with a failing-first
  regression, 230 regular tests, 22 explicit GPU correctness cases and a release
  WASM viewer build. Preserve the public decoder API and attribute layouts.
  Scope: bundled SOG decode; packed LOD and browser worker readback remain
  separate paths.
- [x] Compare three fresh native processes per revision/scene for SOG ownership
  transfer: Apartment decode 77.262 -> 62.109 ms (19.6% lower), JunkShop
  165.399 -> 141.172 ms (14.6% lower), with exact static-pixel equality across
  all repeats. Publish all 160 metrics and pin the four native plots to the
  matched v2 histories on one readable hardware testbed; preserve old reports
  and all 13 plot identities/order. Record startup/tail-frame variability and
  the absence of an overall startup/FPS or regression-gate claim in the
  [evidence](sog-buffer-handoff-evidence.md). Raw samples/images remain ignored.
- [x] Pin 13 focused Bencher dashboard plots after querying all 52 selected
  series for nonempty, correctly dimensioned data and verifying real-scene
  values against saved reports. Rename 40 plotted real-scene metrics while
  preserving benchmark UUIDs/slugs. Re-fetch all six reports and verify all 548
  measurements after renaming. Preserve the existing CPU sorting plot UUID while
  correcting its dimensions. Verify plot order, selections, units and axes;
  document [plot interpretation](../docs/performance-plots.md), quality budgets
  and the need to update pins for new workload versions. Add a regression test
  accepting readable service labels through stable submitted aliases while
  continuing to reject unrelated measurements. Script/workflow checks pass.
  The public headless preview hit Cloudflare security verification (HTTP 403),
  so saved data/configuration are API-verified; rendered dashboard layout was
  not visually reviewed.
- [x] Separate real-scene v2 hardware/environment testbed names from camera,
  resolution, LOD/settings and instrumentation workload identities. Preserve
  saved v1 validation and CPU aliases/baselines. Add readable names for newly
  created testbeds and a reviewed, UUID-preserving rename command for existing
  histories. Verify that project-key CI uploads never request forbidden renames.
- [x] Apply the 14-entry existing-testbed rename plan using a local Bencher user
  API key after the project key correctly rejected renames. Verify all 14 readable
  names with unchanged UUIDs/slugs, all 63 reports with unchanged measurement
  counts/testbed associations, and all 43 thresholds with unchanged models,
  branches, measures and testbed associations. No history was moved or deleted.
- [x] Publish and revalidate six real-scene reports against raw samples and
  returned service values: 80 native metrics for explicit Apartment/JunkShop
  camera candidates from clean revision `d14bb26`, and 468 browser observations
  from clean revision `747332f`. Keep privacy-redacted browser GPU identity on
  `browser-diagnostics`; it cannot pass a hardware regression gate. Add compact
  service metric names with saved full-identity mappings after diagnosing slug
  collisions. Record actual settings, repeat counts, links and limitations in
  the [measurement evidence](real-scene-benchmark-evidence.md).
- [x] Inspect all eight native camera captures. Main views show interiors;
  Apartment's side view is too close to geometry and needs further calibration.
  These are candidate performance workloads, not approved Pixel Eagle references.
- [x] Add verified real-scene suites, three independent native runs/browser
  rounds, raw-sample revalidation, fixture-bound BMF and identity-separated
  Bencher publication. Publish 40 native metrics each for Apartment and
  JunkShop from clean optimized revision `747332f` on Apple M5 Max / Metal.
  These establish initial history, not a passed regression gate or cross-engine
  parity. The automatic native camera fits all bounds and is not an approved
  scene-specific interior view. Retain that history separately from the later
  explicit-camera workloads above.
- [x] Add maintainer-dispatched hardware workflow and declare the custom runner
  label. Runner provisioning/scheduling remains separate; no hosted CPU runner
  is presented as a hardware GPU or mobile testbed. Untrack the previously
  tracked `.env`, retain its safe defaults in `.env.example`, and ignore the
  local credentials file.
- [x] Diagnose missing worker/codec artifacts in the initial browser package;
  package the production SOG worker and separate codec WASM and verify all
  required site artifacts before replay. Retain the failed initial run.
- [x] Implement external Pooch-backed offline fixture preparation from pinned
  local inputs, atomic cache publication, complete hash/reference/range checks
  and machine-readable entrypoints/provenance. Prepare Apartment, JunkShop SH3
  and all seven Playbot sources without adding model binaries to Git. Existing
  offscreen runner smoke-renders both bundled scenes in four views; the existing
  production hierarchy decoder validates seven chunks and 919,279 logical splats
  across LODs. These are compatibility checks, not performance or image baselines.
- [ ] Establish fixed real-scene visual references and repeatable benchmark
  captures, including browser streaming and physical mobile. Resolve recorded
  asset-specific permission evidence before automatic acquisition/publication.
- [x] Inventory local real-scene candidates and prepare external dataset-catalog
  records with pinned upstream revisions, payload hashes and asset-specific
  license evidence. Conflicting/unverified permissions keep promotion to shared
  fixture downloads and release publication pending; code licenses alone do not
  establish dataset permissions. No model binaries are added to Git.
- [x] Add durable agent guidance requiring separate visual and performance
  evidence for affected Gaussian changes, external image references, matched
  benchmark settings and explicit reporting of unmeasured scopes.
- [x] Inventory automatic CPU/pipeline benchmarks, native completed-frame runs,
  browser cadence/milestones and existing shared CPU/cache/activation/bake
  diagnostics. Distinguish existing instrumentation from published regression
  history. Record sort precision/mode gaps and stochastic research separately
  in the [performance TODO](splat-performance-todo.md).
- [ ] Complete phase-scoped CPU/GPU attribution, coverage-aware LOD activation
  aggregation, real hierarchy/network churn, loader/worker parity, controlled
  hardware history and physical-mobile comparisons. Existing streaming and
  counters do not establish complete end-to-end performance coverage.

# Published history cleanup (2026-10-29)

- [x] Merge PR #15, then remove generated visual PNGs from history with a
  path-scoped rewrite. Verify every rewritten commit preserves all other files;
  verify a fresh clone has no target PNGs in branch/tag history. Keep a verified
  external backup bundle, update only the three affected branches and restore
  the original branch protection after publishing with expected-SHA leases.
- [x] Seed three trusted main CPU reports for EPYC 9V45 testbed
  `cpu-v1-d9fe3dac069d1c9a` in run `40781844466`. Recheck the saved PR #15
  measurements: required CPU gate passes in run `40780909343`, attempt 2.
  Preserve the existing 20% threshold. The optional pipeline gate separately
  lacks an EPYC 7763 baseline; its trusted main bootstrap is run `40785729290`.
- [x] Audit subsequent failures: pipeline bootstrap `40785729290` completed on
  EPYC 9V74, so it did not seed the missing 7763 testbed. Latest main CPU run
  `40786175068` passes. Pipeline run `40786175060` has 12 active alerts, 22.8%
  to 33.9% above baseline, despite identical benchmark/renderer sources, lockfile,
  toolchain, CPU model and runner image relative to the bootstrap. Shared-host
  performance variation is a hypothesis, not a proven code regression. Use
  paired base/head measurements or controlled hardware to establish causality;
  do not repeatedly bootstrap or relax thresholds to make the checks pass.

# Pixel Eagle reference storage correction (2026-10-29)

- [x] Follow Bevy's artifact-to-service approach: remove committed visual PNGs,
  calibration captures, local reference registry/creator and golden comparator.
  Keep production renderer scenes, 22 GPU cases and analytic image assertions.
- [x] Upload the reviewed PR #14 captures to Pixel Eagle: browser run 1 and native
  Vulkan run 2; hosted Metal run 10 and DX12 run 11. Configure fixed service
  references and enable automatic reporting.
- [x] Verify authenticated upload and comparison through workflow `40780285426`:
  native run 3 against 2 and browser run 4 against 1 both pass. This reuses the
  source captures to verify integration; it is not an independent rendering run.
- [x] Add Metal/DX12 service reporting when those capture artifacts are present;
  keep PR execution credential-free and reporter execution on trusted code.
- [x] Verify independent merged-commit renders in workflow `40780750849`: all
  four service comparisons pass against the fixed references. Validate 42 Python
  tests, 49 Node tests, Actionlint/Zizmor, and all five actual hosted captures on
  each platform with the simplified analytic checker. Commit `cb4d313` removes
  all 35 tracked visual PNGs and adds an ignore rule to keep captures out of Git.
  Cleanup is in PR #15; its hosted build/render checks run independently.

The following sections record the earlier implementation; Git-based references
and calibration described there are superseded by this correction.

# Gaussian visual correctness implementation (2026-10-29)

- [x] Activate a reviewed inventory of 21 existing raster/packed/turn cases plus
  one physical buffer test, with serial execution and strict count guards.
  Combined local run passes 22/22 on M5 Max/Metal. Fix stale test fixtures:
  conditionally disable pipelined rendering, explicitly select Snapshot for
  expanded sources and initialize the now-required packed-residency resource.
  Preserve production defaults and all existing assertions.
- [x] Add one deterministic five-case native/WASM renderer testbed with real
  sort/draw acknowledgements, completed GPU captures and independent footprint,
  coverage, depth/color and SH assertions. Two native and two real Chromium
  WebGPU runs pass; corresponding PNGs match byte-for-byte within each lane.
- [x] Add strict reviewed-baseline comparison, region assertions, explicit
  two-clean-run reference creation, exact metadata/inventory guards, numeric
  results and retained actual/expected/diff images. Verify blank/drop/order,
  missing reference/capture, corrupt PNG and incompatible metadata failures.
- [x] Add Linux software Vulkan, browser WebGPU, opt-in PR / scheduled native
  Metal/DX12 workflows and manual remote physical-mobile capture. Separate
  privileged Pixel Eagle publication from PR execution, validate bounded source
  artifacts and persist trusted workflow provenance. Document configuration in
  [Gaussian visual testing](../docs/visual-testing.md).
- [x] Validate script suites, native/WASM strict visual Clippy, formatting,
  Actionlint/Zizmor and standalone export/headless dependency isolation locally.
- [x] Seed reviewed clean references for local Metal, browser Metal, hosted
  Vulkan/Metal/DX12 and hosted Chromium WebGPU. Hosted run `40723856559`
  passes native gates on all three backends and all 22 Linux GPU cases.
  Its five browser captures pass semantic checks and repeat exactly; review
  and register those images. Final source commit `954bd68` passes all four
  hosted visual lanes in run `40728561048` and regular CI in `40728561038`.
  Independent Linux ARM64 captures match the hosted browser pixels exactly.
  Linux Chromium requires Mesa Vulkan and Xvfb; watchdog disabling alone did
  not resolve device loss. Windows WARP requires bounded timeout retries
  within the existing 90-second phase; its first sort takes about 57 seconds.
  Preserve original Vulkan references and measured one-level variation bounds
  (channel delta one, mean error 0.002), with boundary rejection tests. Other
  lanes retain zero-error thresholds. A real overlap reversal fails the gate
  and restoration passes. Ignore only Chromium startup duration in repeated
  capture metadata; missing/incompatible references still fail explicitly.
- [ ] Enable authenticated Pixel Eagle reporting after reviewed service reference
  IDs are supplied; verify a live diff and recovery. Project UUID and Actions
  token are configured. Keep automation disabled during explicit upload-only
  bootstrap; the trusted reporting workflow must first reach the default branch.
- [ ] Diagnose performance baselines/alerts independently of visual correctness.
  Final run `40728561266` is blocked by a missing depth-sort baseline for
  EPYC 9V45 testbed `cpu-v1-d9fe3dac069d1c9a`; pipeline run `40728561137` passes.
  Trusted
  main CPU bootstrap `40710957448` succeeded, but pipeline main bootstrap
  `40710964874` alerted on compose: 751010 ns versus upper limit 747848 ns.
  PR CPU run `40723856547` alerts on million-element depth sort (9871200 ns
  versus 9031640 ns limit) and SOG decode (7658700 ns versus 7586400 ns limit).
  Benchmark and algorithm sources are unchanged by this visual work; the cause
  is unproven. Preserve protection, thresholds and reports; measure same-commit
  variance and paired base/head runs before changing performance policy.
- [ ] Run on an identified physical phone/tablet with a reachable supported
  remote browser. No physical-mobile result or FPS improvement is established.

# Render correctness and OSS CI audit (2026-10-29)

- [x] Audit Bevy's Pixel Eagle, scene testbed, WASM and mobile workflows at
  `a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2`; record source links and a staged
  proposal in [Visual regression and OSS CI audit](visual-regression-ci-audit.md).
- [x] Identify existing ignored raster/packed/buffer GPU tests as the first CI
  activation target. Existing LOD/compositing pixel assertions complement the
  performance runner's minimal non-background check; neither is a golden suite.
- [x] Add prioritized capture/assertion, reviewed-baseline, browser execution,
  optional Pixel Eagle and platform/device work to the performance TODO. Check
  current GitHub public-runner, Pixel Eagle free-tier and BrowserStack OSS terms.
- [x] Prove the software-renderer lane, activate selected existing tests and add
  expected-image assertions through the implementation above. Mobile and
  fixed-hardware performance remain separate work.

# Comprehensive splat benchmarks (2026-10-28)

- [x] Capture the prioritized follow-up work and acceptance conditions in
  [Splat performance TODO](splat-performance-todo.md). Streamed SOG already loads
  chunks and refines LOD; whole-file local PLY/bundled SOG is a separate path.
- [x] Promote the reported mobile/SuperSplat difference to P0 in that TODO.
  Record the desktop-only harness (1000x760, DPR 1), generic WASM budgets and
  missing physical-device capture. Track matched-device/backend/quality replays,
  resolution and memory sweeps, input stalls, sustained performance and measured
  mobile defaults. Existing device caps do not establish phone-specific tuning.

- [x] Audit the local 3dgs renderer benchmark and Spark, PlayCanvas Engine,
  SuperSplat and Gaussian-Splat-Lite implementations. Document loading boundaries,
  asynchronous sorting, completed frames, residency and quality comparison limits
  in `docs/benchmark-methodology.md`.
- [x] Add 12 opt-in production-path CPU workloads: PLY, bundled/packed SOG,
  expanded composition, hierarchy parsing, perspective selection with two budget
  domains, pressure relocation, cache admission and resident scheduling. Preserve
  the original five workload identities and headless default graph.
- [x] Validate generated SH3 attributes, covered relocation and budgets outside
  timing. Export hashed PLY/SOG and a two-level hierarchy for viewer acceptance.
- [x] Add separate pipeline Bencher CI/testbeds and PR branches. Keep original
  required checks; new checks require explicit baseline bootstrap and verification.
- [x] Add native offscreen completed-frame measurements, optional radix timestamps,
  raw camera/build/asset identities, percentile metrics and validated captures.
  Actual Apple M5 Max smoke: 400 acknowledged frames with 400 matching timestamps;
  diagnostic builds correctly suppress BMF publication.
- [x] Run the clean optimized native GPU executable at `c25a9c0`: all 1,440
  measured frames acknowledged production sort/draw, four captures passed, and
  24 BMF metrics passed provenance checks. Results remain local; see
  `docs-dev/native-gpu-benchmark-evidence.md` for boundaries and limits.
- [x] Add opt-in browser telemetry and production viewer replay. Distinguish
  loader readiness, encoded draw, refinement settling and app/render/RAF cadence.
  Never label these as GPU execution or first visible pixel time.
- [x] Run fixture/policy/native helper tests, strict pipeline Clippy, Python/JS
  contracts, workflow linting and isolated export/headless boundary checks.
- [x] Verify release WASM and final real browser replays: cold/warm 10k PLY and
  a 20k-budget stream selecting 16,384 coarse splats, with no pending work.
  Evidence and limits: `docs-dev/browser-replay-evidence.md`.
- [x] Resolve independent review findings with regression checks: stale output,
  RAF truncation, unknown-adapter publication and degenerate camera motion.
- [x] Address hosted native telemetry Clippy findings and lint the instrumented
  WASM library directly in CI. Example-only `--no-deps` checks had skipped that
  library. Preserve the bounded worker handoff while cleaning up existing WASM
  result mapping and documenting its intentional enum size/function arguments.
- [x] Complete the full optimized Criterion run and validate exactly 12 finite
  results (`dist/performance-pipeline-expanded-v3`). Local dirty/concurrent
  measurements establish suite acceptance, not publishable regression baselines.
- [x] Complete the pipeline's initial [three-report bootstrap](https://github.com/soraxas/dimensify/actions/runs/40691518690).
- [ ] Seed missing hosted testbeds and verify normal pipeline comparisons. The
  [following run](https://github.com/soraxas/dimensify/actions/runs/40692688818)
  failed for a missing baseline; keep the new checks optional until verified.
- [ ] Diagnose representative real multi-source assets with matched cameras;
  generated fixtures and a single-leaf hierarchy do not establish SuperSplat parity.
- Discovery: a 1 MiB source/cache setting cannot admit even a tiny SOG source:
  production reservation includes a 4 MiB worst-case SH/codebook allowance. Retain
  such runs as failed budget scenarios, never successful loading measurements.
- GPU/browser results remain local; no hosted GPU runner is configured. Cache
  payload bytes are not process RSS or total VRAM. Timestamp spans cover only the
  instrumented sorting/compaction work.

# CPU performance tracking (2026-10-28)

- [x] Choose Bencher CPU tracking on hosted CI; preserve renderer/GPU timing
  instrumentation for a later GPU suite. Same-repository PRs block at 20%;
  fork PRs retain artifacts without publishing.
- [x] Add failing tests for deterministic workloads, complete report ingestion,
  missing baselines and a synthetic 25% slowdown. Confirm reporting tests pass.
- [x] Implement deterministic SH3 decode, 100k/1M sort and 1k/10k LOD workloads;
  verify input correctness and the headless dependency graph. Keep the source
  export boundary check covering the new benchmark package.
- [x] Integrate separate measurement/publication jobs and main-only three-run
  baseline bootstrap. Pin Bencher 0.6.14 through mise and isolate the cache lane.
- [x] Run all five release workloads (3s warm-up, 100 samples, >=10s measurement),
  three input-correctness tests, 18 Python/script tests, focused decoder/sort/LOD
  suites, strict benchmark Clippy, formatting, Actionlint, Zizmor and isolated
  export. The pinned Bencher CLI dry-run accepts the saved output and threshold
  options; server-side adapter parsing is also confirmed by hosted reports below.
- [x] Independently review and fix feature-branch manual runs contaminating main
  history and failed measurement jobs skipping the required gate. Verify the
  added branch-ref test and synthetic 25% regression test fail when expected.
  Forks require the measurement check; publication remains intentionally skipped.
- [x] Commit and publish `9d32eed`; hosted Linux workload validation and all five
  measurements pass. Initial publication correctly fails without configuration.
- [x] Upload that saved hosted result anonymously to create the public Bencher
  `dimensify` project (BMF v0), configure GitHub's `BENCHER_PROJECT` variable and
  validate all five parsed metrics from the live API. Confirm the gate rejects
  the initial report because three historical samples do not yet exist.
- [x] Add a README Bencher badge linking to the public results dashboard and a
  separate live CPU-workflow status badge; document their meaning and setup.
- [x] Claim the project, configure `BENCHER_API_KEY` and verify authenticated
  reporting in the [three-run bootstrap](https://github.com/soraxas/dimensify/actions/runs/40677808141).
  Keep AMD EPYC 7763, EPYC 9V74 and Intel Xeon 8573C histories in separate
  testbeds; hosted allocation varies even on the same Ubuntu runner label.
- [x] Verify a real PR regression: [disposable PR #13](https://github.com/soraxas/dimensify/pull/13)
  deliberately triples decode work. Bencher records 23.12 ms against an 8.09 ms
  baseline (+185.96%), posts a PR report and fails the workflow gate.
- [x] Restore the PR workload and verify the gate passes with computed boundaries
  for all five metrics and zero alerts ([recovery report](https://bencher.dev/console/projects/dimensify/reports/01a18243-5a2e-77e9-831c-3920950b1220)).
- [x] Verify a [fresh normal main run](https://github.com/soraxas/dimensify/actions/runs/40680133536)
  passes all five baseline comparisons with zero alerts. Close the disposable
  PR without merging and delete its branch. Enable and read back main branch
  protection requiring **CPU performance gate** and **Measure CPU workloads**
  with an up-to-date base; preserve the existing administrator bypass.
- Hosted runner allocation includes at least three CPU models. New testbeds
  fail explicitly until seeded; never merge histories or widen the 20% boundary
  to hide hardware changes. A dedicated machine remains the path to stable
  frame-time/GPU comparisons.

# RoboSim restoration

- [x] Reproduce the missing `robot` feature and binary after Bevy migration
  `c3dc6884`; inspect the prior implementation.
- [x] Scope: optional native URDF kinematic viewer, bundled demo, joint and
  camera controls. Retired egui/editor, transport, IK, and physics integrations
  remain dormant. The historical Panda mesh assets are absent from this checkout.
- [x] Implement the feature-gated viewer and local mesh loading.
- [x] Verify loading, joint limits, hierarchy and a single Z-up conversion.
- [x] Build the native binary and check the default dependency boundary.
- [x] Document commands, supported inputs and verification limits.
- [x] Review and reproduce zero STL normals and untransformed Collada normals;
  recalculate imported normals from final geometry.
- Verification details and limits: [native reproduction](../docs/repros/robosim-2026-10-13.md).
- [x] Establish the open-source subtree boundary in `AGENTS.md` and remove
  downstream product references from maintained files.

## Build tooling

- [x] Declare Dimensify's existing workspace-inherited UI dependency in Moon
  metadata, since Moon 2.6.1 does not infer that relationship. Cargo manifests
  and feature selection remain authoritative.

## Standalone collaboration extraction

The active workspace now selects viewer markers, transport, sessions and the hub.
The 0.25 transport prototype is retained in `legacy/transport/` outside the active
graph; its Python/protocol consumers require an explicit migration before use.

Transport owns listeners and connection attempts; sessions supply the handshake
and room policy. Document/camera sharing is explicitly registered per application.
The hub composes a selected schema without owning domain data or asset access.

- [x] Preserve the existing native protocol and listener baseline.
- [x] Resolve the active workspace from an isolated source export.
- [x] Extract transport attempts/listeners without room or domain policy.
- [x] Move session authority, registration, document and presence suites.
- [x] Compose a generic host and move HTTP/TLS tests; local HTTP suites pass.
- [x] Compile the headless browser client to WASM.
- [x] Complete native/browser session acceptance and pre/post-extraction interoperability.
- [x] Run final isolated-export suites and document remaining device limits.

Validation commands and supported behavior belong in [collaboration](../docs/collaboration.md).
The export check first failed because the session package was absent; it now
resolves without parent manifests. New standalone source uses public dependencies.

The final isolated export passes the 66 generic tests and the 23-test server-only
configuration. Native and headless Chromium clients both complete invite, edit,
bidirectional presence, private identity resume and explicit leave. Clippy exits
successfully with style warnings. Browser WebTransport/TLS and full browser/device
acceptance remain outside this milestone; native tests exercise QUIC/fallback.
The HTTP test fixture waits for `/health` readiness instead of TCP acceptance.
The exported RoboSim target also passes a native compile check after narrowing
workspace defaults; no new graphical runtime capture was performed.

## Active crate examples

- [x] Add small window-free consumers for viewer camera setup, UI camera markers
  and transport endpoint validation; reuse the session check and hub executable.
- [x] Expose the two-viewer example with `collab-peer`, preserving the Dimensify
  feature in printed join commands. Document server authority and LAN usage.
- [x] Extend `just tests` to all five active workspace crates.
- [x] Move the pre-migration `custom_widgets` example to `legacy/examples/`:
  workspace testing exposed imports of the retired egui and panel registry APIs.
- [x] Verify the runnable examples, workspace tests and two-viewer acceptance.
- [x] Apply commit-hook formatting and strict workspace Clippy cleanup; rerun
  all 65 session/host tests after simplifying nested conditions.

The workspace passes 67 tests (one deployment test remains intentionally ignored).
Camera setup, UI markers and endpoint examples run successfully; invalid endpoint
input exits unsuccessfully. The headless two-client check passes invite, edit,
bidirectional presence, identity resume and leave against a separate host process.
Both graphical example tests pass, including offscreen two-client edit/follow/resume.
Inspected captures show both peers at revision 2 with the same cube and camera pose
(`/tmp/dimensify-two-peers-2026-10-15/host.png` and `viewer.png`).

The first offscreen attempt timed out before joining; a diagnostic retry passed.
The harness now waits for listener readiness on its actual bound ephemeral port,
runs scene startup before connecting, and reports session/transport errors early.
Its final run passes, but the initial timeout's cause was not conclusively identified.
Missing HUD separator glyphs were replaced with ASCII and recaptured.
Manual interaction in two OS windows, two-machine LAN use and rendered browser
behavior remain unverified; no visible windows were opened.

Commands and coverage boundaries belong in [examples and tests](../docs/examples.md).

- [x] Keep the explicit room/token viewer and add `shared_cube_invite` using short
  viewer/editor codes, sharing its scene and controls. Use standard Cargo/Just
  commands in printed instructions and the example guide.
- [x] Verify both native example builds/help output, invalid-code/argument rejection,
  shared camera tests, the network invite-code/renewal test and isolated export.
  This addition did not rerun GPU captures or browser verification.

## Mainline integration (2026-10-21)

- [x] Integrate mainline build tooling without changing the standalone boundary.
- [x] Verify all 66 transport/session/hub tests and the isolated export; Moon graph,
  planner and environment checks pass. The environment test resolves Rust selectors
  through mise rather than comparing the compiler to the literal `latest` string.
- Full rendered-viewer, browser runtime and remote deployment checks were not rerun.

## PLY and SOG splat viewer

Approved scope: an optional native/browser WebGPU viewer, independent of robot,
physics, session and hosting dependencies. A reusable `dimensify_splat` plugin
owns decoding, validated scene replacement and framing. The example owns the
Open File button, drag-and-drop zone, controls and visible loading errors.
Source coordinates default to right-handed Z-up with one root basis conversion;
Y-up files can explicitly select their source frame. Bundled SOG v2 is supported;
unbundled metadata/image directories and compressed PLY dialects are excluded.

- [x] Package generic renderer and codec dependencies inside the standalone tree;
  remove the inactive pre-migration splat adapter.
- [x] Add failing fixture tests for PLY/SOG decoding, invalid input and bounds;
  implement validated decoding and finite geometry checks.
- [x] Add scene replacement tests including failure preservation and stale results;
  implement the reusable plugin and explicit camera framing.
- [x] Add native Open File and native/browser drop loading through one request path;
  build the viewer with its empty drop zone, status and navigation controls.
- [x] Verify focused tests, native and WASM builds, isolated dependency boundaries
  and offscreen rendering; document commands and remaining runtime limits.

The 13 focused tests, strict Clippy, native/WASM builds and isolated export pass.
Headless browser acceptance and inspected captures verify file selection, PLY
rendering, SOG drop, framing and failure preservation. Native OS interaction and
large captured scenes were initially unverified. See the [reproduction record](../docs/repros/splat-viewer-2026-10-22.md).

- [x] Remove the fixed two-million-Gaussian cap after a valid five-million SOG
  was rejected. Validate declared counts against PLY payload/SOG texture capacity
  and check device storage capacity before cloud allocation. Native decoding of
  the local five-million-Gaussian input and the regression suite pass.
- [x] Verify native/WASM viewer builds and rendering with that larger input.
  Both builds and strict Clippy pass; 15 focused tests pass, plus the optional
  local five-million decode test. Headless browser acceptance loads all five
  million Gaussians without console errors; the rendered capture was inspected.

## Shared Feathers viewer UI

Use one retained Feathers toolbar, drop overlay and status view on native and
WASM. Keep file selection/read/drop adapters platform-specific. The browser
page only hosts the canvas and initializes WASM; it has no parallel viewer UI.

- [x] Verify activation tests expose the old UI's missing Feathers actions.
- [x] Share Feathers controls, source selection, file state and status across targets.
- [x] Replace HTML controls with a minimal canvas host and browser file adapter.
- [x] Verify focused tests, native/WASM builds and headless browser interaction;
  update the viewer guide and reproduction record.

All 17 focused tests, strict Clippy, native/WASM builds and isolated export pass.
Inspected headless browser captures verify the shared controls, PLY/SOG loading,
the local five-million scene and failed replacement. Native OS interaction
remains unverified. See the [Feathers reproduction record](../docs/repros/splat-feathers-2026-10-23.md).

## SuperSplat capability audit

- [x] Compare the active viewer with SuperSplat 3.6.0 source, separating exposed
  behavior, unused renderer APIs and missing capabilities. See the
  [gap audit](../docs/supersplat-gap-audit.md) for revisions, evidence and proposed
  ordering; the audit does not authorize implementing the whole editor.
- Camera damping/fly/surface focus, useful initial framing, packed residency and
  adaptive rendering are distinct gaps. Stochastic transparency needs a new
  render path; existing encoded compositing is not equivalent. Multi-LOD import
  selection is not continuous runtime streaming. Keep future tools optional and
  preserve the shared native/WASM UI and headless dependency boundaries.

## Shared viewer foundation direction

- [x] Revise the planned order to extract existing reusable implementations
  before building missing viewer features; the audit describes the standalone
  example and is not an inventory of every consumer.
- Target ownership: generic GPU kernels/layouts and compact upload primitives
  remain in the maintained renderer fork; an optional camera package owns
  navigation/pose behavior; `dimensify_splat` owns neutral source resolution,
  residency, cache/LOD/activation policy and diagnostics. Consuming applications
  retain authorization, product defaults and domain workflows. The pose/view/lens/bounds core and manifest/LOD targeting are extracted;
  full navigation and shared controls migration remain pending. Source/residency
  migration is completed in the streaming slice below.
- Reconcile renderer/codec/sorter snapshots and Bevy feature/type compatibility
  before migration. Preserve one camera writer, native/WASM Feathers controls,
  existing backend behavior and isolated headless graphs. Stochastic rendering
  and FOV Auto Dolly remain new work after extraction acceptance.

Shared-source extraction now uses one maintained renderer/codec/sorter under
`vendor/`. Original licenses, tools and fixtures are retained. `dimensify_camera`
provides pose/view/lens/bounds APIs and a headless bounds-fit example;
`dimensify_interpolation` owns the bounded playback queue. `dimensify_splat`
exposes manifest validation and LOD targeting without its `render` feature.
The core graphs remain independent of rendering and physics. Full device/navigation composition and shared quality controls remain
pending; this progress does not complete the viewer-foundation milestone.

Verification and remaining runtime limits are recorded in the
[foundation reproduction](../docs/repros/viewer-foundation-2026-10-23.md).

The [camera core](../docs/camera.md) also owns direct pose-transition easing,
smoothed first-person look and Fly velocity response. Consumer input capture,
collision/terrain adapters and the sole final camera writer retain their existing
ownership. A headless Fly example exercises semantic movement and idle drainage;
the local-file viewer now consumes this Fly response as described below.

Walk fixed-step integration, jump/suspension, queued impulses and display
interpolation are also shared. `WalkEnvironment` supplies clearance, terrain and
collision resolution without requiring a physics engine. The headless Walk
example uses an analytic floor. Consumers retain scene geometry, input capture,
mode policy and the final camera writer. Device-input migration and
interactive Walk composition in the file viewer remain pending.

Inspect anchored orbit/pan/zoom, finite smoothing, cancellation and semantic view
requests now live behind `dimensify_camera/inspect`. Its scene interface leaves
surface picking, clipping and collision geometry with consumers. The vendored
motion engine and its provenance move inside `vendor/`; renderer-only extensions
are optional so the inspect graph remains headless. A headless example computes
targets without writing a camera transform. Device capture and interactive viewer
composition remain pending; this is partial viewer-foundation progress.

Mouse inspect integration now uses shared button ownership, gesture latching and
wheel mapping in both consumers. The file viewer forwards to the shared inspect
controller with finite smoothing and keeps its sole final transform writer.
Explicit framing retains its existing margin. Touch capture is recorded below; interactive
Walk remains open; robot integration is recorded below. See the
[foundation reproduction](../docs/repros/viewer-foundation-2026-10-23.md) for
headless browser evidence and remaining runtime limits.

### Local viewer origin, Fly and live file basis

The file viewer starts in Fly at the origin, preserves its view on load, and
exposes Reset, explicit Frame and a pose-preserving Fly/Orbit switch. Fly consumes
the shared momentum/look helpers; Reset and focus/UI suppression clear motion.
File up axis now reorients the current root and recomputes conservative bounds
from original geometry, also updating pending loads without decoding again.
Browser acceptance exercises these controls through real canvas input.

File up axis accepts all six signed axes. Each maps to renderer +Y with a proper rotation, preserving handedness; repeated changes always use original source bounds.

### Direct remote splat resources

The standalone viewer has one shared Feathers Open URL panel. HTTP(S) downloads
retain signed query strings, follow redirects, bound incoming bytes, and publish
through the same latest-selection queue as local files. Axis selection is applied
at read completion. Native uses optional Reqwest/Rustls on a worker; browser uses
Fetch streams and the resource host's CORS policy. This supports direct whole-file
PLY/bundled SOG resources, not provider authentication. Hosted hierarchy streaming is added below.

The URL field supports browser Command shortcuts through a thin input adapter; native OS clipboard support is enabled only by the viewer. Enter opening the panel cannot resubmit a retained URL. Native redirects disable automatic Referer headers so signed query strings are not forwarded.

### Shared production streaming extraction

- [x] Move neutral LOD policy/settings out of presentation ownership.
- [x] Validate the extracted controller, cache/residency and renderer readiness adapters independently.
- [x] Switch both consumers to the shared pipeline and verify hosted-manifest replacement.
- Preserve requested/submitted/displayed ownership, bounded requests and existing renderer backends. Provider authorization, product presets and progress presentation stay with consumers.

- [x] Move neutral source/byte resolution, production scheduling, worker decode,
  cache/budget admission, snapshot/resident/packed adapters and diagnostics into
  `dimensify_splat`; remove migrated duplicate implementations.
- [x] Keep product-independent core graphs headless, preserve explicit WebGPU/WebGL
  feature selection, and support native streaming without selecting browser UI.
- [x] Refresh the gap audit and hosted-hierarchy instructions. Local folder import,
  new codecs, stochastic rendering, Auto Dolly and shared quality panels remain
  later slices.

The [streaming reproduction](../docs/repros/splat-streaming-2026-10-26.md)
records source/artifact identity, unit/codec checks, offscreen renderer handoffs
and real headless browser replacement/worker/axis acceptance. This does not
complete camera/device migration or claim captured-scene/performance parity.

### Shared quality observations

- [x] Move quality/memory aggregation and bounded leaf snapshots into
  `dimensify_splat::diagnostics::quality`; install the same observer for custom
  hosts and the standalone streaming viewer.
- [x] Preserve renderer-ready versus requested counts, labelled allocation versus
  estimate/reservation, bounded evidence, opt-out cleanup and unchanged-report
  change detection. Host presets and presentation remain outside this observer.
- Shared quality panels remain future composition work; a shared report does not
  claim a complete shared settings UI or new rendering modes.

### Current architecture boundary

- [x] Audit the active workspace and module graph, then update the existing
  architecture/overview to separate reusable viewer/session behavior from
  consumer policy and UI. Mark excluded protocol/Python/legacy sources as
  prototype designs rather than supported launch APIs.
- [x] Keep the headless host independent of graphics/robot/physics and document
  opt-in renderer/streaming/viewer layers. Correct the README to describe
  origin/Fly, explicit framing and hosted sources.
- This is a useful extraction boundary, not completion of the simulation
  roadmap. Shared quality panels, full camera/device composition, additional
  formats/rendering modes and Python/physics launch contracts remain open.

### Robot camera integration

- [x] Replace the native robot viewer's duplicate orbit transform math with
  shared inspect orbit/pan/zoom, drag ownership and bounds union. Keep initial
  framing and explicit `F` requests, with one final camera writer after robot
  geometry propagation.
- [x] Verify focus-loss cancellation, held-button ownership on focus restore,
  right-drag pan, zoom/orbit and same-frame framing with the real viewer schedule.
- [x] Keep `dimensify_camera/inspect` optional under `robot`; the default host and
  camera/policy graphs retain their existing dependency boundaries.
- Interactive Walk and shared quality controls remain unfinished. Touch progress
  is recorded below; gamepad expansion is deferred.
  The [foundation reproduction](../docs/repros/viewer-foundation-2026-10-23.md)
  records native offscreen and focused verification evidence.

### Touch-first examples and gallery

- [x] Share stable one/two-finger motion and viewport ownership, preserving
  identity rebasing, UI ownership and interrupted-contact behavior. Keep host
  tap/navigation/joystick policy outside the shared tracker.
- [x] Connect touch input to the standalone Orbit/Fly viewer, suppress synthetic
  mouse duplication, expose touch hints and use 44-pixel toolbar targets.
- [x] Catalog every active feature package, add a cursor playback example and
  provide a static gallery with Browser/Native/Headless filters and a live
  splat-viewer entry point. Browser robot showcases can expand later;
  console examples are labelled as console examples.
- Prioritize touch and real phone/tablet acceptance. Gamepad expansion is
  deferred; existing downstream gamepad behavior is retained during extraction.
- [ ] Replay picker, Orbit/Fly, two-finger pan/pinch, toolbar ownership and
  contact interruption on physical phones/tablets; browser-emulated touch does
  not establish device compatibility.
- [x] Discover Cargo examples automatically and move card overrides into each
  owning manifest. Opt browser targets into one WASM/worker packaging command;
  keep native/headless targets honest and check catalog freshness on export.
- [x] Add live camera and two-tab collaboration examples through the same
  discovery/build path; preserve shared controls and local host prerequisites.
- [x] Extract the tested standalone device adapter into opt-in camera viewer
  navigation. Splat loading/modal adapters only publish bounds and capture state;
  the example shell composes shared Feathers controls and a flow-layout parent.

- [x] Add the example-owned native/WASM connection form: validate host/name/code
  before replacing a connection; Retry uses the existing endpoint-bound private
  resume credential. Keep schema, authority and transport ownership unchanged.
  Verify form input/camera capture, invite-code join and retry in a real browser.

### Standalone build, CI and public gallery

- [x] Pin mise Rust/tooling and enable the local Mr Boxington wrapper. Retain
  Just as the standalone task entrypoint and separate compiler cache lanes.
- [x] Add first-party formatting, strict native/headless/WASM Clippy, focused
  tests, export boundaries and workflow linting to pull-request/main CI.
- [x] Package all three browser examples and SOG worker assets into a complete
  static gallery; reject incomplete builds and preserve project-relative URLs.
- [x] Add headless static-site acceptance, PR artifacts and Pages deployment
  of the tested artifact after all checks pass. Static hosting does not supply
  a collaboration authority; an HTTPS client needs its own secure endpoint.
- [x] Verify release WASM packaging, strict native/headless/WASM Clippy,
  focused tests and the project-subpath gallery. Replay browser camera,
  collaboration and splat/hosted-worker acceptance headlessly. Physical-device
  touch acceptance remains tracked above.
- [x] Group routine GitHub Actions updates into one weekly PR; group security
  fixes separately and avoid enabling an unrequested Cargo update stream.
- [x] Expose lint, tests, RoboSim validation, native example builds and export
  verification as named CI steps while retaining the aggregate local commands.
- [x] Separate unoptimized native verification from interactive development:
  the [first hosted run](https://github.com/soraxas/dimensify/actions/runs/40438666025)
  passed lint/tests but exhausted its hour rebuilding
  optimized Bevy dependencies for RoboSim. Keep release gallery quality intact.
- [x] Make localhost transport fixtures use blocking accepted sockets explicitly;
  inherited nonblocking mode can drop delayed headers on macOS.
- [x] Enable Pages and publish the [example gallery](https://cs.tinyiu.com/dimensify/).
  Inspect the [successful hosted checks and deployment](https://github.com/soraxas/dimensify/actions/runs/40445725236),
  verify public asset responses and WASM content types, and replay camera
  orbit/reset/fly and browser touch controls on the deployed site headlessly.


### Experimental stochastic Gaussian transparency

- [x] Correct stale stochastic scope/follow-up wording after review and verify
  the direct-draw contract with a failing count mutation independently of sorts.
- [x] Apply protocol-conditional raw JSON schemas: matching transparency,
  mode-specific acknowledgements and zero-sort/direct-draw evidence. Verify
  17 invalid schema mutations fail before the fix and both actual Playbot raw
  reports pass afterward. Preserve the implicit sorted mode of older saved
  reports during runtime validation without altering their recorded evidence.

- [x] Add explicit Sorted / Stochastic settings to the viewer toolbar, native
  argument and browser query. Sorted remains the default. This is stochastic
  rasterization, not MCMC training; automatic motion switching is deferred.
- [x] Use deterministic coverage, opaque color and reverse-Z depth writes.
  Direct source draws skip radix dispatch and stale indirect counts; preserve
  actual draw acknowledgements and resume sorting on return to Sorted.
- [x] Apply the opt-in policy to current and pending streamed clouds. Verify
  binary coverage, repeatability, overlap/order distribution, mode restoration
  and real projected packed LOD handoff with the production renderer.
- [x] Separate native stochastic benchmark protocol and workload identity from
  sorted history; reject reports that claim sorted acknowledgements for
  stochastic draws. Expose the choice in trusted native LOD benchmark dispatch.
- [x] Validate focused Rust suites, script/format/workflow checks, native and
  WASM compilation/Clippy, and all 25 GPU correctness cases on Apple M5 Max
  Metal. Run the external hash-verified Playbot fixture in both modes: 1,224
  completed frames and six inspected nonblank captures. These unoptimized smoke
  reports are ineligible for publication and establish no performance gain.
- [ ] Measure optimized matched sorted/stochastic runs and inspect Bencher and
  Pixel Eagle results before making quality/performance claims. Browser runtime
  acceptance and physical-mobile performance remain separate follow-up work.
- [ ] Address fixed-sample grain, stable source identity through LOD remapping,
  cold pipeline-switch gaps and general translucent-mesh compositing before
  treating the experimental mode as a quality-equivalent replacement.

Details: [stochastic transparency](../docs/gaussian-stochastic-rendering.md).
Captures remain local artifacts; no reference images or model assets enter Git.
