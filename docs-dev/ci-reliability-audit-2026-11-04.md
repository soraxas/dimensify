# CI reliability audit, 2026-11-04

## Integration

The failed-run audit covered 74 failures since October 28: 32 visual reports,
33 CPU/pipeline checks, five general CI runs and four visual correctness runs.
These include historical failures already addressed; they are not 74 current
regressions.

Main `7b57f3e` still contained a duplicate `Tour` definition. PRs 29-31 had
merged into feature branches after their parent PRs had already merged. The
integration PR brings the reviewed `feat/viewer-feathers` stack onto main.
Local verification: 289 viewer tests pass, 45 GPU/platform tests explicitly
ignored, WASM viewer-perf Clippy, script checks and formatting pass.

## Visual reports

Source run [41765811777](https://github.com/soraxas/dimensify/actions/runs/41765811777)
expired while waiting for approval and produced no captures. Its reporter tried
to download nonexistent artifacts. Reporting now inventories actual lanes before
scheduling downloads and preserves a separate failed-source verdict. Successful
sources must still contain both required Linux lanes.

The Vulkan variation predates the viewer changes. Captures from the same commit
`5d956e7` in source runs
[41388704186](https://github.com/soraxas/dimensify/actions/runs/41388704186) and
[41299698248](https://github.com/soraxas/dimensify/actions/runs/41299698248) differ
despite matching recorded scene, settings, adapter, driver and toolchain:

| Case | Changed pixels of 65,536 | Maximum channel delta |
| --- | ---: | ---: |
| anisotropic | 200 | 1/255 |
| overlap-front | 343 | 1/255 |
| overlap-reverse | 343 | 1/255 |
| sh-front | 412 | 1/255 |
| sh-reverse | 412 | 1/255 |

The newer `72c5c03` captures match the second variant exactly. This establishes
same-commit cross-run variation, not its cause. CPU-dependent llvmpipe arithmetic
is a hypothesis; historical CPU details were not recorded. New capture metadata
includes host CPU/flags, runner image and allowlisted renderer environment.
Linux native and browser jobs now capture twice and retain exact difference
statistics. These diagnostics do not change Pixel Eagle acceptance or references.

Two local Apple M5 Max/Metal captures were byte-identical for all five cases;
all five analytic assertions passed. This does not establish Linux determinism.
Evidence remains in ignored `dist/visual-native-reliability-*` outputs and the
downloaded audit artifacts; no images enter Git.

Hosted follow-up on revision `f18c933`:
[41821522667](https://github.com/soraxas/dimensify/actions/runs/41821522667)
passed native Vulkan GPU/analytic checks and browser WebGPU analytic checks.
All five native repeats and all five browser repeats were byte-identical. Both
workers reported AMD EPYC 7763, runner image `20261026.354.1`. Native driver,
scene and settings metadata match the reviewed reference; all five native PNGs
also match the `41388704186` reference variant exactly. This supports
cross-worker variation but does not prove which CPU/compiler operation causes
the other variant. Repeat diagnostics run after primary analytic assertions,
so a repeat failure cannot prevent those assertions from executing.

## Performance verdicts

Missing hardware history, withheld credentials and measured alerts have distinct
causes. Fork/Dependabot PRs retain measurements as artifacts and explicitly skip
credentialed publication. Other missing credentials remain configuration failures.
Main hardware cohorts still require three real bootstrap measurements before a
historical comparison exists; results on another CPU cannot fill that gap.

The paired pipeline alert in
[41750042693](https://github.com/soraxas/dimensify/actions/runs/41750042693)
measured cache admission at 72.871 us versus a 19.5143 us base mean on the same
Xeon 6973P-C runner. The base processes measured 18.413, 20.483 and 19.647 us;
the candidate interval was 60.590-83.149 us. Relevant cache/benchmark sources did
not change. Candidate and base Criterion iteration counts differed, and the
consuming v1 closure includes input destruction.

An opt-in diagnostic harness separates borrowed admission, batching policy and
destruction costs. It does not replace v1 or enter automatic Bencher publication.
The existing 20% gate and history remain intact. A local diagnostic result cannot
by itself explain that Linux alert or prove a production performance improvement.

Three sequential optimized diagnostic processes on Apple M5 Max (Rust 1.98.1,
Criterion 0.5.1, one-second warmup, three-second measurement, 20 samples) produced
these point estimates in microseconds:

| Timed routine | Process 1 | Process 2 | Process 3 |
| --- | ---: | ---: | ---: |
| Legacy consumed input, SmallInput | 40.429 | 30.789 | 23.614 |
| Borrowed admission, SmallInput | 14.908 | 24.392 | 15.793 |
| Borrowed admission, PerIteration | 4.4184 | 6.9617 | 3.9728 |
| Input destruction, PerIteration | 3.4530 | 4.1150 | 3.2550 |

All processes ran the same executable after compilation finished. Background
desktop services were not isolated. The results demonstrate sensitivity to
input lifetime/batching and cross-process variation on this machine; they do
not establish a Linux cause. Raw samples, full intervals, logs, harness SHA-256
and environment remain in ignored `dist/cache-admission-diagnostic/`. An earlier
exploratory process overlapping a Clippy build is excluded from this table.

Remaining work: correlate new Linux CPU/repeat evidence with Pixel Eagle diffs;
reproduce paired alerts with controlled scheduling/allocator traces; obtain
measured history for uncovered main hardware cohorts. Do not auto-approve image
references, silently rename a workload or reseed valid history to clear failures.
