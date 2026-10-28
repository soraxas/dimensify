# Splat performance tracking

Dimensify tracks CPU operation latency with Criterion and Bencher, and provides
separate native GPU and browser viewer replays. Hosted CI measures CPU operations;
real rendering measurements run on a machine with a hardware GPU.

## Results and badges

Open the public [Bencher results dashboard](https://bencher.dev/perf/dimensify)
to select a branch, CPU testbed and workload, then compare measurements over time.
The README's **Bencher CPU results** badge opens that dashboard. Its **CPU
performance** badge shows the latest main-branch performance workflow status and
opens the workflow logs. These links require no API key; the key belongs only in
GitHub Actions secrets. A [verified main report](https://bencher.dev/console/projects/dimensify/reports/01a18248-22c9-75e4-a90b-1707c5b4a94f)
provides a direct example of all five measurements and baseline comparisons. Each automatic PR comment
links to its own report and plots. The `main` branch requires **CPU performance
gate** and **Measure CPU workloads**, enabled after a live regression/recovery
check and successful main comparison on 2026-10-28.

!!! note "Keep measurement boundaries separate"
    The CPU dashboard measures operations, not FPS. The native runner measures
    serialized completed frames. The browser replay measures loading, streaming
    state and frame cadence. None of these is a substitute for the others.

## Run locally

```sh
mise install
mise exec -- just ci-perf
mise exec -- just perf-cpu
```

Results and environment metadata land in `dist/performance/`. The directory must
be new; use `mise exec -- just perf-cpu dist/performance-next` for another run.
Local runs require no Bencher account and never upload automatically. Compilation
finishes before measurement. Each workload uses 3 seconds of warm-up, at least
10 seconds of measurement and 100 Criterion samples, with throughput optimization
(`opt-level=3`, thin LTO and one codegen unit).

| Workload | Timed operation |
| --- | --- |
| `sog-decode-v1/65536` | Decode seven prepared lossless WebP textures and expand 65,536 splats, including SH3 attributes and output allocations |
| `depth-sort-v1/100000` | Sort 100k retained positions, alternating two view axes; includes copying the returned order |
| `depth-sort-v1/1000000` | Same operation for 1M positions |
| `lod-targets-v1/1000` | A five-view near/far/near LOD cycle over 1k leaves with reused hysteresis state |
| `lod-targets-v1/10000` | Same camera cycle over 10k leaves |

Inputs are generated from fixed values and seeds; there are no scene downloads
or private datasets. Fixture generation, texture encoding, manifest construction,
sorter construction and correctness checks happen outside measurement. The LOD
time covers five policy updates, not one frame or the full streaming scheduler.
Decode excludes ZIP parsing, network transfers, GPU upload and rendering.

The Bencher Criterion adapter records estimated latency in nanoseconds with
confidence bounds. This is Criterion's slope/mean estimate, not P95/P99 latency.
Do not interpret these estimates as frame-time percentiles.

## Production loading and LOD suite

```sh
mise exec -- just ci-perf-pipeline
mise exec -- just perf-pipeline
```

The optional `pipeline` feature exercises production code without requiring a
GPU. Its renderer dependencies are isolated from the default headless CPU suite.
Results go to a new `dist/performance-pipeline/` directory.

| Workload | Timed operation |
| --- | --- |
| `pipeline-ply-v1/10000`, `/100000` | Binary SH3 PLY parsing and expanded allocation |
| `pipeline-sog-bundle-v1/65536` | ZIP extraction, WebP decode and expanded SH3 SOG loading |
| `pipeline-sog-packed-v1/65536` | Packed SOG decode and retained texture construction, without expanded attributes |
| `pipeline-compose-v1/65536` | Reorder and copy all expanded attributes across 64 ranges |
| `pipeline-metadata-v1/1000`, `/10000` | Parse and validate a spatial hierarchy |
| `pipeline-lod-selection-v1/1000`, `/10000` | Eight perspective views through targets and selection, with CPU/GPU source budgets and hysteresis |
| `pipeline-relocation-v1/pressure` | Six recorded pressure views through source planning, selection and covered relocation |
| `pipeline-cache-admission-v1/1024` | Admission and eviction policy across 1,024 modeled source entries |
| `pipeline-scheduler-v1/resident` | Eight resident streaming ticks with production scheduling and modeled renderer acknowledgements |

Fixture construction and correctness assertions are outside timing. The cache
fixture uses empty payload sentinels and modeled byte costs: it measures policy,
not payload destruction or memory bandwidth. Resident scheduling excludes network,
decode, GPU upload and renderer completion. LOD and scheduler values represent a
whole camera cycle, not an individual frame. Loader timings include output
allocation and destruction; they exclude fixture encoding and network transfer.

The separate [Pipeline CPU performance workflow](../.github/workflows/performance-pipeline.yml)
publishes all 12 metrics using `pipeline-cpu-v1-*` testbeds. The original five
required measurements retain their identities and thresholds. Bootstrap the new
workflow explicitly on main, then verify a normal comparison before requiring its
checks. Pipeline PR histories use `pipeline-pr-N` so publication cannot reset the
original suite's `pr-N` branch. Local runs do not upload.

## Completed frames on a GPU

```sh
mise exec -- just perf-gpu --output dist/performance-gpu
mise exec -- just perf-gpu /path/to/scene.sog --output dist/performance-gpu-scene
```

The native runner uses the production Bevy splat plugin, an offscreen 1280x720
target, SH3 and radix sorting. Four cases cover static/moving main and near-side
views. Each frame updates the app and waits for GPU completion, with one frame in
flight. Warm-up requires acknowledged sort/draw work and a stable pipeline. Each
case samples at least 240 frames and two seconds of completed work. Camera poses,
raw intervals, adapter/build/source identities and validated PPM captures accompany
median/P95/P99 summaries in `raw.json` and `bmf.json`.

`completed_frame` includes app update, submission and GPU waiting; `app_update_wall`
also includes render scheduling and submission. These are not isolated CPU/GPU
pass times or display FPS. `--gpu-timestamps` additionally records available radix
and compaction timestamps, not total rendering time. Keep this instrumentation
mode consistent between runs. Decode and renderer initialization are recorded
separately from steady-state samples. This runner loads one complete PLY or bundled
SOG; use the browser replay for streamed hierarchies.

An omitted source generates 65,536 splats. Supply a representative real asset for
production diagnosis. `--smoke` permits diagnostic runs and suppresses BMF output.
Dirty, mismatched or unoptimized builds also suppress publication metrics. Use a
clean checkout and the bench profile for performance comparisons. Output directories
must be new. No local GPU results upload automatically; keep hardware, driver,
resolution, dataset and settings in a dedicated Bencher testbed before publication.

## Browser loading and streaming

The [browser replay guide](../docs-dev/browser-replay.md) describes building an
instrumented release viewer and replaying real picker/URL, camera and loading
flows. It records cold/warm runs, time to loader readiness and first encoded draw,
refinement settling, source-cache activity, pending work and CPU stage counters.
Bounded histories include app, encoded-render and browser RAF intervals with
median/P95/P99 summaries; these are cadence, not completed GPU frame latency.

Generate public fixtures for local replay:

```sh
mise exec -- cargo run --locked --profile ci -p dimensify_perf --features pipeline --bin perf-fixtures -- dist/perf-fixtures
```

This writes 10k/100k PLY, a 65k SOG bundle and a 65k/16k two-level hierarchy, with
SHA256 file identities. Use `--splat-budget`, `--cache-mib`, `--source-mib` and
`--concurrency` to exercise constrained budgets. A source that cannot fit may
never reach first draw: retain its raw diagnostics as a failed scenario rather
than reporting fast loading. A single-leaf fixture does not establish large-scene
LOD scaling, eviction recovery or parity with another renderer.

See [benchmark methodology](benchmark-methodology.md) for the reference audit and
the conditions required for renderer comparisons.

## Enable Bencher

1. Start with a real benchmark upload; Bencher can create an unclaimed project
   without an account or API key. Follow the **Claim this project** link printed
   by the CLI to sign up or claim it with an existing account, then create an API
   key. Do not create a second project if an initial upload already created one.
   Dimensify's first hosted report is available in the public
   [dimensify project](https://bencher.dev/perf/dimensify). It uses BMF version 0,
   matching the pinned Criterion adapter and latency threshold configuration.
2. In GitHub repository Actions settings, add variable `BENCHER_PROJECT` with
   the project slug or UUID (`dimensify` is already configured for this repository)
   and secret `BENCHER_API_KEY` with a user API key or scoped project key.
   Never commit the key or paste it into logs.

    Anonymous upload is only for initial onboarding: anyone can post to an
    unclaimed project. Claim the project before relying on its regression gates.
    After claiming, uploads require a key. The automatic CI publisher deliberately
    requires credentials rather than silently falling back to anonymous uploads.
3. Merge the workflow and benchmark suite onto `main`. Run **CPU performance**
   manually on `main` with **bootstrap** enabled. This measures three independent
   runs and saves three reports, even though they share a commit.
4. Run the workflow normally on `main`. Confirm all five workloads have baseline
   boundaries. A new CPU model, OS release, architecture, Rust version or workload
   version requires a new baseline. Hosted CPU allocation can vary, so bootstrap
   each testbed encountered rather than comparing unlike machines.
5. Verify a same-repository PR passes. On a disposable branch, deliberately add
   enough work inside a timed operation to exceed 20%, and verify the gate fails.
   Remove the slowdown, rerun and verify it passes before merging.
6. Only after that verification, require **CPU performance gate** and
   **Measure CPU workloads** in the `main` branch rules. Keep existing required
   checks. Requiring measurement also protects fork PRs, whose publication is
   intentionally skipped. Initial setup and later missing
   baselines fail explicitly; they do not indicate a performance pass.

Manual publication and bootstrap are only permitted on `main`. Bootstrap accepts
missing boundaries while seeding history; it is not a successful regression
comparison. Do not use
bootstrap as a way to approve a regression or automatically reseed on a PR.

## CI behavior

The [performance workflow](../.github/workflows/performance.yml) runs on every
main push, PR targeting main and manual dispatch. It checks out the exact PR
head, tests input correctness, measures without upload credentials, then starts
a separate publication job. Bencher is pinned through mise, and the performance
build has its own compiler cache lane.

Same-repository PRs inherit main's baseline at the PR base commit. The gate fails
if any workload exceeds the mean of the latest ten eligible main reports by more
than **20%**, with at least **three historical reports** required. The script also
requires a computed boundary for every workload, since Bencher can accept a
report before enough history exists. It rejects missing, duplicate, non-finite
or incomplete measurements and propagates publication failures.

Bencher posts its PR comparison and check using a token restricted to checks and
PR reporting. The workflow gate remains authoritative if a Bencher check is
created before report validation finishes. Fork PRs get test and benchmark
artifacts only; their publication job is skipped. A maintainer must reproduce
them on a same-repository branch for a regression gate. No `pull_request_target`
workflow executes fork code with credentials.

Artifacts are retained for 30 days and include raw Criterion logs, environment
metadata and Bencher responses. Metadata records the exact commit, dirty state,
CPU, architecture, OS, runner image, Rust version, profile and workload version.
CPU/OS/architecture/toolchain fingerprints isolate testbeds. Version workload
names and the testbed prefix when changing workload semantics or profile settings.

!!! warning "Hosted runner noise"
    The 20% boundary is an initial tolerance, not a guarantee of stable hardware.
    Investigate a failure with a fresh run and its environment metadata. Smaller
    changes remain visible in history. Do not silently widen thresholds or blend
    CPU models to make a failing result pass.
