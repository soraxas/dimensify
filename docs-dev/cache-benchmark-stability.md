# Cache benchmark stability

The PR #34 alert measured 54.836 us against a 43.623 us baseline mean. Three
processes of the unchanged base measured 17.844, 66.436 and 46.588 us. A fresh
workflow passed without changing code or the 20% threshold. This demonstrates
variation; it does not identify the Linux cause or invalidate every alert.

The v1 consuming closure times full replay destruction as well as admission.
Criterion's SmallInput batching changes the live input population with the
sample iteration count. Borrowing separates teardown; fixed-size batches also
bound the input population. Real evictions inside admission remain timed.

## Local experiment

Apple M5 Max, macOS 26.7.1, Rust 1.98.1, Criterion 0.5.1, optimized bench profile.
Each process uses 100 samples, three seconds of warmup and ten seconds of target
measurement per workload. Compilation finishes first. Background desktop load
is not isolated, and these results do not establish hosted Linux repeatability.

Five initial processes compared the existing diagnostic boundaries:

| Timed routine | Process estimates (us) | Process CV |
| --- | --- | ---: |
| Consumed input, SmallInput | 11.851, 10.627, 10.056, 9.764, 11.761 | 8.88% |
| Borrowed input, SmallInput | 6.417, 6.109, 5.654, 5.555, 6.388 | 6.69% |
| Borrowed input, PerIteration | 3.599, 4.962, 5.014, 3.681, 3.528 | 18.30% |
| Full teardown, PerIteration | 2.710, 2.653, 2.662, 2.643, 2.608 | 1.39% |

Borrowing one input alone did not stabilize admission, so it was not promoted.
The next experiment adds `cache-admission-v2/1024` with eight inputs per batch;
the three older admission controls remain available in the same executable.
Five processes of that executable produced:

| Timed routine | Process estimates (us) | Process CV |
| --- | --- | ---: |
| Consumed input, SmallInput | 10.639, 11.052, 10.634, 10.091, 11.042 | 3.68% |
| Borrowed input, SmallInput | 5.538, 5.795, 5.420, 5.425, 5.804 | 3.42% |
| Borrowed input, PerIteration | 3.612, 3.501, 3.473, 4.644, 3.415 | 13.85% |
| Borrowed input, fixed eight | 4.193, 4.114, 5.820, 4.077, 4.043 | 17.27% |
| Full teardown, PerIteration | 2.654, 3.900, 2.632, 2.608, 2.653 | 19.56% |

The fixed-eight candidate did not improve local repeatability and is not
promoted into the regression gate. Its worst relative confidence-interval width
was about 53%; the report marks it variable. Load averages were recorded, but
they do not establish the cause of individual outliers. This is a
measurement-boundary experiment, not a production cache optimization or a speed
comparison between interchangeable metrics.

Final experiment executable SHA-256:
`ad40fc49a618ae2a7f0c3c0139f97edee72274dd3aeec47f6c4408e41788b6db`.
The runner verifies the executable and relevant source hashes remain unchanged
during timing. Local source was a recorded dirty checkout of `b1f25ec`; these
are diagnostic observations, not Bencher publication baselines.

Raw logs, samples, estimates, executable hashes and environment records remain
in ignored `dist/cache-stability-local/` and `dist/cache-stability-fixed-eight/`.
The initial experiment's provisional metric labels are defined by its retained
`experimental-harness.rs` and `experimental-runner.py`; they were not published.

## First hosted Linux experiment

[Run 41969229764](https://github.com/soraxas/dimensify/actions/runs/41969229764/job/127053318550)
completed successfully on a clean checkout of `570b54b`, Ubuntu runner image
`20261026.354.1`, AMD EPYC 9V74, four logical CPUs and Rust 1.98.1. Three
processes used the same optimized executable and settings as above. Downloading
the `cache-stability` artifact and recomputing its report from all 1,500 raw
samples reproduced the saved summary:

| Timed routine | Process estimates (us) | Process CV | Widest relative 95% CI |
| --- | --- | ---: | ---: |
| Consumed input, SmallInput | 469.389, 440.525, 457.169 | 3.2% | 3.78% |
| Borrowed input, SmallInput | 19.542, 19.464, 19.348 | 0.5% | 3.43% |
| Borrowed input, PerIteration | 5.037, 5.059, 5.046 | 0.2% | 0.55% |
| Borrowed input, fixed eight | 19.935, 20.006, 19.746 | 0.7% | 0.81% |
| Full teardown, PerIteration | 7.138, 7.313, 7.119 | 1.5% | 0.69% |

All five routines met the diagnostic criteria in this run. The fixed-eight
candidate is promising on this host, but the adverse desktop experiments remain
part of the evidence. One hosted run does not establish stability across runner
allocations or explain the original alert. Differences between these routines
measure different timing boundaries and input populations; they are not cache
speed improvements, and their values cannot replace the v1 history.

Executable SHA-256:
`af4612dfad658d09636aa1bb640d9e98d689a2204c63513316b1396e112945c8`.
The artifact retains the source hashes, raw samples, estimates, build logs and
environment. A downloaded copy is in ignored `dist/cache-stability-linux/`.

## CI adoption

Pipeline CI runs three processes in an independent diagnostic job. Results
appear in its job summary and in the `cache-stability` artifact. Invalid or
incomplete samples fail the experiment; variable timing is
retained with an explicit diagnostic flag, without retrying until green.

Keep the v1 Bencher gate and its history unchanged while collecting further
Linux evidence across runner allocations. A new gate needs separately versioned
metrics, matching real baselines and a documented migration. A single hosted
pass is insufficient.
