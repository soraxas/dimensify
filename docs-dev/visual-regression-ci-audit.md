# Visual regression and OSS CI audit

Audited 2026-10-29 against Bevy commit
`a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2`. This is an implementation backlog;
no Pixel Eagle project, token, device-farm account or new CI lane was configured.

## What Bevy does

The [example workflow](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/workflows/example-run.yml)
runs named example scenes, collects PNGs and traces, and uploads artifacts.
PRs run the macOS Metal job; Linux Vulkan and Windows DX12 jobs run on main and
merge groups. Linux installs Mesa and uses Xvfb. Only Windows explicitly selects
its backend in this workflow; job names alone do not attest an adapter.

The [Pixel Eagle workflow](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/workflows/send-screenshots-to-pixeleagle.yml)
creates a run with OS/branch/commit metadata, registers PNG file hashes, uploads
unknown images, then requests automatic comparison against main for the same OS.
Both reported missing images and differences produce warnings **and exit 1**. Required
branch checks are separate configuration; these files do not prove merge blocking.
The workflow does not pin a baseline commit or specify comparison tolerances.
A missing token skips comparison, so a successful job need not mean images passed.

For PRs, a separate [report workflow](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/workflows/example-run-report.yml)
consumes the originating run's screenshots without executing PR code with the
service token. A failed comparison can post a comment. Its deliberate-rendering-
change label suppresses that comment; it does not approve or refresh a baseline.

Borrow the artifact/report separation, named scenes and reviewable diffs. Improve
the details: validate HTTP status and response schema, exact capture inventory,
archive paths and artifact provenance. Bevy's current script lacks these checks;
absent/null comparison fields can be interpreted as zero differences. Obtain PR
identity from trusted GitHub metadata, rather than a file supplied by PR code.

## Other Bevy tests worth adapting

- [Headless app tests](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/tests/how_to_test_apps.rs)
  and [isolated system tests](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/tests/how_to_test_systems.rs):
  inject input/resources, advance `App::update`, and assert state without a window.
  Extend our existing policy tests for gesture-driven camera/LOD changes and
  cancellation ordering. These remain fast complements to actual pixel tests.
- [General CI](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/workflows/ci.yml):
  unit tests across Linux/macOS/Windows, separate WASM/target checks, docs and
  targeted Miri validation. Borrow a small supported-feature/platform matrix;
  upstream's no-std/atomics/Miri jobs are not automatically useful requirements
  for this viewer. Check opt-in streaming/render paths and dependency isolation.
- [CI testing plugin](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/crates/bevy_dev_tools/src/ci_testing/mod.rs):
  configurable frame events and optional fixed time. Its
  [ScreenshotAndExit handler](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/crates/bevy_dev_tools/src/ci_testing/systems.rs)
  waits for capture completion. Still assert the saved file: save errors can be
  logged before a successful exit.
- [Named testbeds](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/examples/testbed/helpers.rs):
  cycle through small scenes and advance after capture. The helper waits 100
  frames after scene changes; that does not establish pipeline/asset readiness
  or deterministic rendering. Reuse Dimensify's existing readiness controls.
- [WASM smoke tests](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/start-wasm-example/tests/wasm_example.spec.ts):
  observe a success message and retain a screenshot. These do not compare expected
  images. The [validation workflow](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/workflows/validation-jobs.yml)
  selects Chromium/Firefox and puts expensive WASM/mobile validation in merge groups.
- [Mobile screenshot scripts](https://github.com/bevyengine/bevy/blob/a62cce8c05ceacd3bd102e678c83e2b42e5dfbe2/.github/start-mobile-example/specs/screenshot.js):
  useful device-automation scaffolding, but a delayed screenshot does not assert
  correctness or establish physical-device performance. Mobile build/simulator
  installation coverage also does not establish phone-browser WebGPU coverage.

Dimensify uses Bevy 0.19 dependencies. Adapt APIs to the locked version; the audited
upstream development checkout is a reference, not a dependency upgrade proposal.

## Our existing coverage and actual gaps

| Existing capability | Boundary / next step |
| --- | --- |
| [Raster handoff tests](../crates/splat/src/streaming/runtime/raster_handoff_tests.rs) | Real offscreen pixel assertions already cover coarse/fine replacement, frustum edges, near plane, camera return, compositing fallbacks, resize/unload, transparent/opaque order and camera isolation. GPU cases are ignored by ordinary CI. |
| [Packed handoff tests](../crates/splat/src/streaming/runtime/packed_handoff_tests.rs) | Existing ignored integration tests exercise residency after CPU eviction, scratch lifetime, projection fallback/coverage, scene ownership and packed/projected refinement. Select and run them explicitly. |
| [Turn coverage tests](../crates/splat/src/streaming/runtime/turn_coverage_tests.rs) | Three ignored pixel tests check immediate coverage after a 180-degree turn for packed, paged and compacted paths. |
| [GPU buffer tests](../crates/splat/src/renderer/gpu_tests.rs) | Existing ignored tests verify physical buffer behavior; complementary to image assertions. |
| [Real-leaf bound reproduction](../crates/splat/src/streaming/runtime/raster_handoff_tests/producer_bounds.rs) | Existing ignored pixel test needs an external `SOG_REPLAY_DIR` fixture. Do not run every ignored test blindly; establish licensed, hashed input availability first. |
| [Native performance runner](../tools/perf/src/bin/gpu.rs) | Fixed time, completed-work synchronization, sample-matched sort/draw acknowledgements and capture completion already exist. Its [image validation](../tools/perf/src/gpu/validation.rs) only checks dimensions and at least one non-background pixel; it is not a golden-image suite. |
| [Browser performance runner](../scripts/perf-browser.cjs) | Render readiness and replay telemetry exist; saved screenshots are not expected-image assertions. |
| [Ordinary CI](../.github/workflows/ci.yml) | Native tests omit ignored GPU cases. The gallery compiles/packages WASM and checks serving/navigation; its GPU-free package check does not execute splat rendering. |

The first task is to activate selected existing raster tests on a verified CI
adapter. Extend them with reviewed appearance baselines and browser execution.
Keep headless checks and dependency isolation intact.

Source inventory: eight raster, ten packed and three turn tests give 21
self-contained ignored integration cases. A candidate invocation for a
software-adapter feasibility run is below; **it was not executed in this audit**.
Verify the actual selected count, adapter and results before adding a required job.

```sh
mise exec -- cargo test --locked --profile ci \
  -p dimensify_splat --features test-support --lib \
  streaming::runtime::raster_handoff_tests \
  -- --ignored --skip producer_bounds:: --test-threads=1 --nocapture
```

The buffer test needs its own filter. The nested producer-bounds reproduction and
separate bounds audit require external fixtures; blanket workspace `--ignored`
is unsuitable. These raster modules exclude WASM, so native activation leaves
browser correctness as separate work.

## Proposed capture and assertion contract

Use one small render testbed with an exact manifest of named cases. Start with
existing helpers and fixtures; extract reusable capture/readiness functions rather
than adding a second renderer. Fix seed, time, camera, splat order, background,
viewport, physical resolution, SH degree, exposure and tone mapping. Require
asset upload, compiled pipelines, current-camera sort/draw and bounded capture
completion. Record scene/build hashes and backend/adapter/driver/browser identity.

| Case family | Assertions |
| --- | --- |
| Single anisotropic/rotated Gaussian | Expected center, footprint orientation, extent, falloff and opacity; off-center coverage must be present. |
| Overlapping translucent Gaussians | Known depth/color relationship, camera reversal and order changes; a visible scene alone is insufficient. |
| SH and projection | View-dependent color, perspective/orthographic and near/frustum-edge support under fixed color handling. |
| Storage and optional paths | Matched decoded/packed/snapshot/resident output; compaction and projection-cache parity where supported. Record intentional differences explicitly. |
| LOD lifecycle | Coarse, transition and settled captures; per-region coverage during replacement, camera return and constrained-budget eviction. Preserve existing lifecycle assertions. |
| Multi-view and teardown | Camera isolation, resize, scene replacement/unload and absence of stale pixels. |

Combine semantic assertions with tolerant image comparison. Record pixel error,
changed-pixel fraction and region coverage; calibrate thresholds using repeated
captures on each lane. A whole-image average can hide a missing small cloud.
Capture the render target/canvas without UI; never mask missing rendered content.
Real licensed scenes supplement analytic fixtures after the small suite is stable.

Require every expected image, correct dimensions, decode success and matching
manifest identities. Retain expected/actual/diff images, numeric results and logs.
Prove the suite fails for deliberately blank output, a dropped cloud, reversed
ordering and a missing capture, then verify recovery.

Pin references to reviewed immutable artifacts/commits and rendering settings.
Native and WASM need separate baselines; OS alone is insufficient. Validate
baseline compatibility before comparison. Missing references need an explicit
bootstrap state; never silently approve them. Baseline changes must show old/new
images and receive deliberate maintainer review. Do not let each main run
automatically bless an unreviewed visual change.

## Proposed CI lanes

| Lane | Initial cadence | Acceptance |
| --- | --- | --- |
| Headless policy/codec tests | Every PR | Existing fast checks and isolated export remain required. |
| Linux software Vulkan correctness | Every PR, after feasibility spike | Run selected ignored buffer/raster tests serially and analytic images. Pin/record Mesa and actual adapter; fail if no adapter or no capture. Software timing cannot publish hardware performance. |
| Browser renderer correctness | Every PR for small fixtures | Execute the packaged WASM viewer, await readiness and compare canvas output. Establish supported software WebGPU flags/backend on the chosen runner before promising this lane. Add supported WebGL fallback coverage separately. |
| Native Metal / DX12 | Main, schedule and manual initially | Small identical scene inventory with platform-specific references and actual adapter records. Promote stable useful checks to PRs later. |
| Physical Android / iOS browser | Manual repro first, scheduled after device access | Device/backend/resolution identity, load/refine/gesture/resize captures and sustained responsiveness. Desktop emulation is separate coverage. |
| Hardware performance | Controlled trusted machine | Existing Bencher metrics, independent repeat runs and per-device histories; image acceptance accompanies timing. |

Keep distinct headless/native/WASM caches and bounded parallelism. Upload failure
artifacts with short retention. Measure lane duration before expanding the matrix.
Use a selected test inventory rather than blanket `--ignored`: some existing
tests require external assets. Add narrow compile checks for supported feature
combinations, and headless injected-input replays where policy bugs can be
asserted without rendering. Preserve current standalone export validation.
Fork PRs must receive local correctness results without service credentials.
Add optional privileged Pixel Eagle reporting only after the local verdict exists:
bind input to source repository/workflow/run/head SHA, validate archives and never
execute PR code or artifact content in that job. Distinguish skipped publication
from passed comparison; service errors must not become a green visual verdict.

## OSS infrastructure options

Checked 2026-10-29; provider terms can change.

- [GitHub Actions](https://docs.github.com/en/billing/concepts/product-billing/github-actions):
  standard hosted runner usage is free for public repositories. Larger runners
  are billed even for public repositories. Use standard Linux/macOS/Windows for
  correctness where compatible; do not treat variable hosted hardware as a stable
  GPU performance baseline. Artifact/cache storage has separate allowances.
- [Pixel Eagle pricing](https://pixel-eagle.com/pricing/): the free public-project
  tier lists 10,000 screenshots and 100 comparison tokens monthly, pixel comparison
  and one-month retention. FLIP is listed on paid tiers. Size the scene matrix
  against actual token accounting before choosing every-PR service comparisons.
  Begin with local comparisons plus artifacts; add this review interface when a
  project/token is available. No special free OSS grant was established.
- [BrowserStack OSS program](https://www.browserstack.com/open-source): advertises
  desktop/mobile testing, Live/Automate/Percy and five parallel sessions for accepted
  projects. Confirm eligibility, relevant device/browser WebGPU access and capture
  behavior before designing a required lane. Shared remote devices need validation
  before using their timings as a regression baseline; local phones remain useful.

## First implementation slice

1. Prove selected existing ignored raster/buffer cases on a standard Linux software
   adapter; record features, driver, duration and failures. Use serial execution.
2. Add deterministic analytic cases, immutable reference metadata and local image
   assertions, including deliberate failure/recovery verification.
3. Execute the WASM render path in CI with the same small case inventory and
   platform-specific references; preserve the tested gallery artifact deployment.
4. Add optional Pixel Eagle artifact reporting and explicit baseline review, then
   expand Metal/DX12 and physical mobile coverage based on observed stability.

Tracked in [Splat performance TODO](splat-performance-todo.md).
