# LLM Agent Guidelines

## Scope

- This entire subtree is an independently usable open-source component. Never
  mention downstream proprietary products, their names, branding, repository
  paths, or integration details anywhere in its source, manifests, comments,
  documentation, examples, or agent instructions. Keep downstream-specific
  guidance outside this subtree; describe shared behavior in Dimensify terms.
- Keep Dimensify viewer-first and backend-agnostic.
- Optional simulation backend is feature-gated (`nox-backend`).
- Prefer adding new APIs through small, stable layers.
- Terminology: use **stream** for scene commands, **telemetry** for sim output, **backend/plugin** for simulation runtime.
- Transport owns connections/listeners; session owns room authority and registered
  document/presence channels; hub composes schemas and hosting. Keep domain policy
  and asset authorization in consuming applications.
- Active transport/session packages select `client` and `server` features.
  Camera sharing is opt-in through session registration. Default headless graphs
  must exclude renderer, robot and physics dependencies.
- Ordered simulation streams, replication, discovery/matchmaking and WebRTC/P2P
  remain future work. The old protocol/Python and `legacy/transport` prototypes
  are excluded from the active workspace; their flags are not current APIs.
- Docs should use MkDocs Material style (callouts like `!!! note`, `!!! warning`, Mermaid diagrams when helpful).
- This file follows the AGENTS.md open format; keep new agent guidance in this file.

## Workflow

- Maintain `docs-dev/migration-checklist.md` and update it after each step.
- Add new decisions, constraints, and discoveries to the checklist.
- Keep docs in `docs/` concise and human-readable, with good examples and explanation.

## Conventions

- Default to ASCII text.
- Prefer small, focused changes.
- Avoid breaking WASM builds unless explicitly requested.

## Validation

- For viewer changes: run native or WASM build as appropriate.
- For Python bindings: ensure module import works and API surfaces are documented.
- For collaboration changes: run the focused suites and isolated export check
  documented in [collaboration](docs/collaboration.md). Browser compilation alone
  does not verify runtime connection, synchronization or resume behavior.

## Build and CI

- mise owns tool versions and Mr Boxington activation; Just owns standalone
  commands. Use `mise exec -- just <recipe>` in noninteractive shells.
- Follow [build and hosting](docs/ci.md) for checks and the static gallery.
  Keep Rust and wasm-bindgen pins aligned with their Cargo/toolchain inputs.
- Preserve separate headless/native/WASM cache lanes and job permissions.
  Publish the tested gallery artifact; do not commit generated binaries.

## Gaussian visual and performance regression checks

- For changes to Gaussian loading, LOD, sorting, shaders, rendering or quality
  policy, identify the affected visual fixtures and performance scenarios before
  editing. Use [visual testing](docs/visual-testing.md) and
  [benchmark methodology](docs/benchmark-methodology.md) for commands and scopes;
  track missing coverage in [the performance TODO](docs-dev/splat-performance-todo.md).
- Verify visual correctness and performance separately. Run relevant analytic/GPU
  assertions and inspect Pixel Eagle comparisons for rendered-output changes.
  Keep reviewed image references in Pixel Eagle and captures/diffs in artifacts;
  never commit generated reference PNGs or auto-approve changed baselines.
- Compare optimized baseline/candidate performance at matched fixtures, camera,
  physical resolution, SH, quality, backend and hardware. Retain raw samples and
  provenance, inspect Bencher results for covered metrics, and investigate alerts
  before claiming an improvement. Do not relax thresholds or reseed baselines to
  conceal regressions; document intentional quality/performance trade-offs.
- Existing CPU/pipeline history does not establish full GPU, loading, frame-time
  or physical-mobile coverage. Distinguish counters, elapsed stages, encoded draw,
  GPU completion and presentation; unavailable metrics remain unavailable.
  Headless/software rendering can verify correctness but cannot establish phone
  performance. Report checks actually run, artifact/revision, results and remaining
  gaps; do not claim broader coverage than the evidence supports.
- Keep checks proportional to affected behavior. Prose-only changes do not need
  a renderer build or performance capture.

## Agent benchmarking workflow

Benchmark first, then change code, then re-measure against the same baseline.
The reusable tooling is `just perf-ab` ([interleaved comparisons](docs/performance.md#interleaved-baseline-candidate-comparisons)),
on top of `just perf-scenes` and the native `just perf-lod` / `perf-gpu` runners.

- **Fixtures:** real scenes come from the `inlier-data` cache via a prepared
  `fixture.json` (see [real-scene preparation](docs/performance.md#real-scene-fixture-preparation)).
  Reuse a fixture from an earlier ignored `dist/real-scenes-*/*/fixture.json`
  when its `root` still exists; never commit fixture binaries or captures.
- **Choose the right path.** Bundled `.sog`/`.ply` fixtures exercise the
  whole-file loader; `sog-v2-hierarchy` fixtures (`lod-meta.json`) exercise
  streaming. State which one a number came from; they are not interchangeable.
- **Baseline:** build it from a ref with `perf-ab site --ref <ref>`; build the
  candidate from the working tree. Record both `build.json` WASM hashes and
  confirm they differ before running.
- **Plan:** copy `tests/performance/ab-plans/streamed-lod-example.json` to the
  ignored `dist/` tree and edit it. Include at least localhost, one throttled
  condition and the constrained preset when transport, scheduling or budgets
  change: localhost bandwidth hides byte-count trade-offs.
- **Run in the background** (each browser run is about 4 minutes). Do not build,
  test or run other heavy jobs while it measures; edit code only. Run the
  summarizer after it completes.
- **Read the timeline, not only the totals.** Per-run `browser/raw.json`
  `observations` (100 ms cadence) show ready splats, pending downloads and
  worker counts over time; `final.worker` and `final.cpu` attribute worker
  startup, decode, delivery and admission. Diagnose with these before editing.
- **Reference renderers:** read PlayCanvas engine and SuperSplat source from
  local clones under `3rdparty/engine` and `3rdparty/supersplat` (add
  `/3rdparty/` to `.git/info/exclude`; never commit or vendor them). Record
  their revisions when citing behavior. Never claim a SuperSplat comparison that was
  not run with matched asset, camera, viewport/DPR, budgets and endpoint.
- **Report** a table of baseline vs candidate per condition, the runs and
  revisions used, regressions as well as gains, and the remaining gaps (for
  example physical mobile, GPU completion or matched cross-renderer endpoints).
  Record durable findings in `docs-dev/` evidence notes and the
  [performance TODO](docs-dev/splat-performance-todo.md).
