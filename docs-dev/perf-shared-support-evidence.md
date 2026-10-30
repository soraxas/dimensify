# Shared benchmark support verification (2026-10-30)

Flat GPU and packed LOD benchmark runners previously duplicated process queries,
renderer setup, completion waits, screenshot readback, PPM output and hashing.
They now share native-only helpers in `tools/perf/src/offscreen.rs` and
`tools/perf/src/process_usage.rs`. Each runner still owns its workload plugins,
resource inventory, report descriptions and publication decisions.

The extraction preserves the 60-second plugin readiness deadline, 30-second
GPU wait/readback deadlines, manual 60 Hz clock, RGBA8 sRGB target and screenshot
conversion. Flat rendering retains optional encoded compositing; packed LOD
retains required HDR/encoded compositing and production streaming telemetry.
Process observations remain outside timed updates. The flat report retains its
additional nullable `unavailable_reason` field.

## Verification

- Baseline: `249a60063845b948667cdf2648f5089fa88d8c26`.
- Candidate: the shared-helper extraction based on that revision, before commit.
- 47 existing focused Rust tests passed before and after extraction.
- Strict native Clippy passed for all performance targets with `lod` enabled.
- The GPU-feature library compiled for `wasm32-unknown-unknown`; native helpers
  are excluded there. The dependency emitted an existing dead-code warning.
- Native binaries built with the `ci` profile. The macOS linker emitted its
  existing compact-unwind-size warning.
- Apple M5 Max / Metal captures used 320 x 180 physical pixels, four-sample camera
  periods, two warmup frames, GPU timestamps and `--smoke`. Each phase still
  required at least two seconds of completed work.
- Flat: all four deterministic SH3 fixture captures were byte-identical
  (`main-static`, `main-moving`, `side-static`, `side-moving`). Baseline/candidate
  recorded 588/600 completed frames across those phases.
- Packed LOD: all three cached inlier-data Playbot captures were byte-identical
  (`static`, `moving`, `near-far-return`). Baseline/candidate recorded 444/508
  completed frames. Fixture identity:
  `3a40bceb8ffa4af2f64051ab9cc192c36ffdc869fbd68aa1301b3e973d493d74`.
- Both LOD raw reports passed the existing JSON schema and saved-capture
  dimension, RGBA hash and non-background-pixel verification. Both runners
  retained resource checkpoint phases, keys, value types, method descriptions,
  capabilities and unavailable-metric declarations.

Local raw reports, logs, captures and comparison results are retained outside
Git in `/tmp/dimensify-shared-support-evidence/`. No reference image was approved
or uploaded, and no generated image or fixture was committed.

!!! note
    These unoptimized smoke runs verify extraction correctness. Timing and
    memory observations vary between processes; they were not uploaded to
    Bencher and do not establish an improvement or mobile performance.
