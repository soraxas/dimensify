# Smooth camera validation

The viewer now eases Frame requests for 400 ms and offers a six-second Tour/P.
Both use `CameraPath` with absolute `Time<Real>` timestamps and quintic easing.
The tour rotates 0.7 radians around the current focus and returns exactly.
Manual orbit/fly damping is unchanged; direct input interrupts playback.

## Checks

- 56 camera tests pass with `viewer`, including equal elapsed-time poses at
  30/60/120 FPS, pivot/lens preservation, exact endpoints, interrupted playback,
  camera loss/replacement, and small positive lens/distance values.
- Camera Clippy with all targets and warnings denied passes.
- Optimized instrumented WASM viewer build passes; script CI and formatting pass.
- Independent review found observation-coverage, stale-status and endpoint-rounding
  issues; each was addressed and reviewed again.

## Local browser evidence

Build base: `0bc6f0d`; modified source, release profile, `splat-viewer-perf`.
Packaged WASM SHA256:
`627732bc4292563e5b61e0c7f6ca0d6eb5db11853fa1b09cea83100c72e67545`.
The ignored `dist/smooth-camera/site/build.json` retains source-file and packaged
file hashes. Chromium 153.0.8010.12, headless, 1000x760 physical canvas, DPR 1.
The active WebGPU adapter is privacy-redacted; no independent GPU attestation.

The hash-verified inlier-data Apartment SOG fixture contains 661,466 splats;
asset-set identity is
`sha256:62928973ffb0721f868eaf41289040b3d4a7271fc2d0fd342d757c617c19a4d3`.
This is local validation using already prepared data. Its cached license record
has conflicting evidence, so captures/models were not published externally.

- `tiny-timing`: one successful tour, 49 pose observations, maximum position
  error 0.0000031 world units.
- `apartment-light`: three successful same-session warmed tours, 169 intermediate
  observations, exact endpoint and pointer interruption assertions pass.
  Median across per-tour app interval medians: 18.5 ms. Median per-tour P95:
  21.6 ms. Encoded-render interval median: 18.4 ms. Maximum observation gap:
  145.7 ms. Maximum observed position discrepancy: 0.000103 world units;
  the explicit tolerance accounts for f32 precision at scene coordinates.
- `apartment-final`: the final runner (WASM hashing deferred until after timing)
  passes three tours with 162 observations. Median per-tour app median: 17.9 ms;
  median per-tour P95: 24.5 ms; encoded interval median: 17.9 ms. Maximum
  observation gap: 305.7 ms. The served WASM hash matches the package hash above.
  Offline revalidation with normalized quaternion angular error <= 0.0001 radians
  also passes all three retained traces (`revalidation.json`). Variation between
  these runs is not an optimization comparison.
- `apartment-captures`: a separate successful capture-only tour, 45 observations;
  cadence metrics are discarded because screenshots disturb delivery. Midpoint
  and endpoint images were retained for inspection.

Initial `tiny-ready` and `apartment` attempts failed observation-coverage checks.
The former captured mid-tour; the latter transferred full GPU/frame histories
through CDP while sampling. The retained frame histories showed that main/render
updates continued through the polling gap. Timing now polls only camera data,
retrieves full histories after the tour, and hashes observed WASM URLs after
measurement. The original failed traces remain alongside the successful runs.
No coverage threshold was relaxed to obtain a pass.

## Limits

These are scoped camera correctness and cadence diagnostics, not matched
baseline/candidate optimization evidence, loading benchmarks, GPU completion,
display FPS or physical-mobile results. The auto-framed exterior Apartment view
is not an authored representative interior-quality fixture. Continuous-tour
screenshots are inspection evidence, not fixed-time golden assertions. Existing
analytic Pixel Eagle references are unchanged; no new comparison was claimed.

The full pan/zoom/near-far LOD/eviction replay still needs authored timed paths and
matched reviewed views before performance publication. The existing mouse replay
continues to exercise real input separately. No diagnostic metrics were uploaded
to Bencher.

## PR #27 review follow-up

The revised diagnostic requires one stable second with no streaming downloads,
generations or pending work before each timed round. It captures the visible pose
at pointer-down to assert continuity when the tour is cancelled. Additional native
regressions cover P replacing a Frame ease, invalid Frame endpoints and focus loss
mid-motion. All 55 camera tests and eight diagnostic helper tests pass, along with
focused Clippy and an optimized instrumented WASM build.

A fresh tiny-SOG correctness run (`dist/smooth-camera/review-tiny/`, ignored) passes
with 56 sampled poses and maximum position error 0.00000293 world units. The
quiescence and cancellation assertions pass. This tiny scene is correctness
coverage; the earlier Apartment timing observations above have not been rerun
under this stronger warmup requirement and remain historical diagnostics.
