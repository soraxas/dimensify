# Viewer settings validation (2026-10-30)

The shared native/WASM viewer now exposes the existing Gaussian quality and
resource controls. This is a configuration and correctness change; these
measurements do not establish a rendering speedup or parity with another viewer.

## Revisions and scope

- Baseline: `f645de1a0b23184d08ae6b57e9f752063c9bdc61`.
- Candidate: `e73d24648c3a2040d08a1dadfcf61f93caed1919`.
- Baseline WASM SHA-256:
  `4efcd01c78b3448f1d874c96e8ab7a6d7a7918d542e4cc4a4b32c0d876bb84c1`.
- Candidate WASM SHA-256:
  `4a7befdf32f1b66162e954b80b364f6e8f3cc1db7fd8f6555f3583d65da2605a`.
- Clean optimized `splat-viewer-perf` packages, Chromium `153.0.8010.12`,
  Apple M5 Max / Metal, macOS 26.7.1, 1000 x 760 physical pixels, DPR 1.
- Cached inlier-data `playbot-lod-v1`, asset identity
  `sha256:3a40bceb8ffa4af2f64051ab9cc192c36ffdc869fbd68aa1301b3e973d493d74`.
  No fixture payload or capture is committed.
- Three fresh browser processes per revision/preset, each with cold and warm
  seven-step input replay. Default order is baseline then candidate; constrained
  order is candidate then baseline. Processes are grouped, not interleaved.
- Settings panel closed during timing; shipping quality, packed backend, SH3,
  camera protocol and resolution match. No concurrent local builds or render
  captures ran during the measurement suites.

Raw samples, package manifests, captures, fixture hashes and service reports are
retained under ignored `dist/viewer-settings/` in the feature worktree. Publication
recomputes metrics and verifies saved fixture/build/capture provenance.

!!! warning "Replay and metric limits"
    Mouse input is real browser input. Intermediate camera poses differ between
    processes, including substantial pan displacement in one default round.
    Moving and near-side observations are diagnostic history, not controlled
    performance comparisons. Static-main and final Frame/reset cameras match
    exactly. App/renderer-encoded/RAF intervals are not completed GPU frame time,
    presentation FPS or pure CPU execution time. WASM capacity is allocated
    linear-memory capacity, not live heap usage or total GPU memory.

## Browser observations

Values below are medians of three process-level observations; app P95 is the
median of each process's P95, not pooled samples. Static-main uses the exactly
matched camera. Cold readiness is the existing renderer-ready milestone, not
first visible presentation.

| Preset / metric | Baseline | Candidate |
| --- | ---: | ---: |
| Default cold readiness (ms) | 206.500 | 224.200 |
| Default cold static-main app median / P95 (ms) | 16.900 / 27.300 | 16.600 / 24.300 |
| Default warm static-main app median / P95 (ms) | 16.900 / 32.400 | 16.200 / 30.500 |
| Default sampled main WASM capacity peak (bytes) | 94,109,696 | 96,272,384 |
| Constrained cold readiness (ms) | 203.600 | 205.200 |
| Constrained cold static-main app median / P95 (ms) | 16.400 / 21.900 | 16.250 / 23.600 |
| Constrained warm static-main app median / P95 (ms) | 16.000 / 27.000 | 15.700 / 27.300 |
| Constrained sampled main WASM capacity peak (bytes) | 72,024,064 | 73,793,536 |

Cold readiness samples are 256.400 / 203.300 / 206.500 ms for baseline and
261.500 / 224.200 / 207.000 ms for candidate. Retain the higher candidate median
and memory capacity; this small grouped sample does not establish their cause.
Constrained cold readiness samples are 208.900 / 202.400 / 203.600 ms for
baseline and 205.200 / 202.300 / 221.200 ms for candidate. Keep the higher cold
static-main P95 and approximately 2.5% higher sampled WASM capacity visible too.

All 24 matched static-main/reset image pairs have identical RGB pixels in the
1000 x 500 scene region at `(0, 150)`, excluding toolbar/status UI. Final cameras
and displayed counts match; this does not approve divergent intermediate views.

Each report contains 215 diagnostic metrics (860 observations total), including
loader readiness, app/encoded/RAF percentiles, LOD/cache/source counters and
scoped memory. Publication revalidated every report against raw evidence and
recorded it on `browser-diagnostics`, with no hardware regression-gate claim:

| Preset | Baseline | Candidate |
| --- | --- | --- |
| Default | [Report](https://bencher.dev/console/projects/dimensify/reports/01a18cd6-0745-75c9-b4f0-39eec1db764c) | [Report](https://bencher.dev/console/projects/dimensify/reports/01a18cd6-19c2-737c-89b7-ff4db5ef5722) |
| Constrained | [Report](https://bencher.dev/console/projects/dimensify/reports/01a18cdc-08c9-750e-908a-d482d977ca1a) | [Report](https://bencher.dev/console/projects/dimensify/reports/01a18cdb-fa0a-778c-879e-80e4f859abc3) |

## Visual and functional checks

- 249 regular splat tests, 11 decoding tests and four quality tests pass.
  Ignored GPU/external-fixture tests are separate from those totals.
- All 26 explicitly selected Metal GPU cases pass: 25 raster cases and one
  resident-buffer case. The new CPU-sort transition test fails with the old
  compacted indirect draw count and passes with the full CPU permutation draw.
  Production CPU-sort tests cover stationary clouds and late sorted buffers.
- Native viewer build, headless compilation, native/WASM strict Clippy,
  formatting/workflow lint, 150 Python tests and 30 visual-helper tests pass.
- The new real-input browser acceptance script passes on the clean candidate
  package, including backend/cache reload, old-scene retention, failed reload
  retry, failed-file recovery, camera preservation and narrow touch ownership.
  Existing file/camera/gallery browser acceptance also passes.
- Independent follow-up code review found no remaining material issues.
- [PR CI](https://github.com/soraxas/dimensify/actions/runs/41056035742) and
  [analytic native/browser visual CI](https://github.com/soraxas/dimensify/actions/runs/41056035798)
  pass. Physical phones and native Windows/Linux interactive panels were not
  exercised locally.

The configured Pixel Eagle references reject both current main and candidate
because their `settingsHash` differs from the fixed reference. No reference was
approved or changed. Separate, unapproved uploads from validated main/PR CI
artifacts allow a matched-identity comparison:

- [Browser comparison: passed](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/46/compare/45).
  All five PNGs are byte-identical.
- [Native Vulkan comparison: differences](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/44/compare/42).
  Per-image differences affect 204 / 351 / 351 / 412 / 412 RGBA channels out of
  262,144, each by exactly one level out of 255. Candidate pixels match the
  previous main run (`f1f11489df23d636ee6b7716fe5b1a5294c10a2c`, workflow
  `41042165775`) exactly, demonstrating that this variation predates this change.
  This is still a failed exact Pixel Eagle comparison, not a reference approval.

## Remaining work

- Investigate the unresolved PR #21 CPU cache-admission
  [alert](https://bencher.dev/console/projects/dimensify/reports/01a18f55-9ef2-70d5-9871-7078820d7dd2)
  on `0fc8fdb`: 123.82 us versus a 14.22 us same-runner baseline, 17.064 us upper
  limit. The benchmark/replay/cache sources match main, but allocator/teardown,
  code generation and runner contributions remain unproven. Retain the failed
  report and 20% threshold; browser diagnostic observations do not clear this
  separate CPU gate. See the performance TODO for the investigation scope.
  A later [pipeline report on `38b0a6f`](https://bencher.dev/console/projects/dimensify/reports/01a191f3-d3d8-7648-85c0-884f37c96d34)
  passes at 14.194 us versus base samples 14.777/14.103/14.328 us. This does not
  explain the earlier spike or establish a performance improvement.
- Make motion replay assert recorded camera poses before treating moving-view
  measurements as controlled comparisons.
- Review the existing Pixel Eagle settings identity drift and native one-level
  variation before any explicit reference update; preserve failed reports.
- Measure open-panel overhead, each optional quality/backend setting, multiple
  real scenes, thermal behavior and physical mobile hardware separately.

## PR review follow-up (2026-10-31)

This section records the source tree committed as
`36ac79af3c8d520624de594a78b6be1780d39e5c`. Its local browser package was built
before that commit, on modified `81a3820`; it is not a clean build of `36ac79a`.
Local `dist/` artifacts are ignored and are not distributed with the repository.

Review fixes prevent Apply from promoting settings or replacing a pending scene,
give same-frame scene requests priority, and preserve live settings on ordinary
load completion. Storage fields now use constructors/read accessors, wasm32 cache
bytes cannot wrap, and mixed projection layouts publish limits per scene. Toolbar
activation and disabled states, global-default fallback, and browser failure
cleanup have stronger checks.

Validation passes: 254 regular splat tests (45 GPU tests remain separately
ignored), five quality tests, the focused pure-WebGL renderer override test,
native/WASM strict Clippy, native and optimized WASM viewer builds, 150 Python
tests and 32 visual/harness helper tests. The renderer fallback test actually
runs with WebGL enabled and WebGPU disabled. The viewer always enables WebGPU;
its unreachable WebGL-only branches are removed in the next follow-up.

Expanded real-input browser acceptance holds an initial source load, verifies
Apply is disabled and live compaction survives readiness, and passes the existing
backend reload, failure recovery, camera and narrow-touch checks. Evidence lives
in `dist/viewer-settings/review-validation/` and `review-browser/`; the precommit
review package in `review-site/` is marked modified on base `81a3820`, WASM SHA-256
`307cb762f5f9721a8aa13904f5fcc3f2c000aea93e38de1753b2682277b4ce6b`.
Independent source review found no remaining material issues.

The timed results above remain measurements of `e73d246`, not new performance
evidence for these lifecycle/diagnostic fixes. No new speedup or visual-reference
approval is claimed.

## Whole-file selection correction (2026-10-31)

The earlier local checks missed Apply while a whole-file decode was pending
alongside a displayed stream. Marking the PR ready overstated that verification.
PR #21 is back in draft. A regression using the production file request system
and committed `tiny.ply` fails before the fix because Apply cancels that decode.
The fix preserves file selection, blocks Apply through the deferred commit, and
releases the load guard when a replacement entity disappears. Separate tests
cover the deferred-commit window, UI activation and removal/retry.

Fresh local verification passes: 258 regular splat tests, five quality tests,
native/WASM strict Clippy and formatting. The 45 ignored GPU/external cases were
not executed by this regular test command. Independent source review found no
remaining material findings in this patch.

Native and optimized WASM builds were made from clean commit
`6b43865438522f5a7293103c36d4ed0f7085e396`. The package records `sourceState: clean`
and WASM SHA-256
`69ab4c8669a0d9b36c5fb7bfc51fc1ec478488eaca743aaa4e1aeb3888a5a520`;
the file hash was checked before browser acceptance. Chromium `153.0.8010.12`
passes the real-input settings suite against that exact package: held streamed
load, disabled Apply, live settings, backend/projection reload, rollback, retained
camera and narrow touch ownership. The whole-file race itself is reproduced by
the deterministic native regression, not a delayed browser file-decode test.

Reproduce on that code revision, with the pinned tools and browser installed:

```sh
mise exec -- cargo test --locked --profile ci -p dimensify_splat --features viewer --lib --test quality --quiet
mise exec -- cargo clippy --locked --profile ci -p dimensify_splat --features viewer --all-targets --no-deps -- -D warnings
mise exec -- cargo clippy --locked --profile ci -p dimensify_splat --features viewer-perf --target wasm32-unknown-unknown --lib --no-deps -- -D warnings
mise exec -- just ci-format
mise exec -- cargo build --locked --profile ci -p dimensify --features splat --example splat_viewer
mise exec -- python3 scripts/perf_scenes.py web-build --checkout "$PWD" --output "$PWD/dist/viewer-settings/whole-file-site"
```

Use an absent output directory for packaging. In one terminal serve the package:

```sh
mise exec -- python3 -m http.server 8765 --bind 127.0.0.1 --directory dist/viewer-settings/whole-file-site
```

Then run acceptance in another terminal and stop the server afterward:

```sh
mise exec -- node scripts/check-splat-settings.cjs http://127.0.0.1:8765/ dist/viewer-settings/whole-file-browser
```

Local logs are in ignored `dist/viewer-settings/whole-file-validation/`, package
metadata in `whole-file-site/build.json`, and captures/results in
`whole-file-browser/`. These artifacts are not included in a repository clone;
the commands regenerate them. This follow-up does not rerun performance suites,
approve Pixel Eagle references, establish physical-phone behavior or claim that
CI on the new revision has passed.
