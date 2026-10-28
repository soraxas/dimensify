# CPU performance tracking

Dimensify uses Criterion workloads and Bencher to track CPU operation latency
between commits. The suite runs on hosted GitHub Actions runners without a GPU.

!!! note "CPU timings, not rendered frame times"
    These results do not measure FPS, frame latency, browser performance or GPU
    pass time. Those need separate native/browser workloads on stable GPU
    hardware. The existing optional `sog-gpu-timing` instrumentation is retained
    for that later work.

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
