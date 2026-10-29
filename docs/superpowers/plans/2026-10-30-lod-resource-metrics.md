# LOD resource metrics implementation plan

> **For agentic workers:** Use the parallel-agent workflow for independent collectors, then integrate and review their shared reporting contracts.

**Goal:** Exercise production packed LOD projection and track browser GPU buffers,
GPU pass samples, optional worker profiles and real-device evidence.

**Architecture:** Reuse the production SOG scheduler and renderer, the existing
bounded timestamp recorder and external inlier-data fixtures. Native runs wait
for completed frames; browser replay preserves continuous rendering and delayed
GPU sample attribution. Profiling has a separate diagnostic cohort.

**Tech stack:** Bevy/wgpu, Rust, Playwright/CDP, Python BMF aggregation, Bencher.

**Spec:** [Measurement boundaries](../../performance-resources.md), extended by
the user's instruction to implement the remaining gaps.

## Constraints

- Model/image/binary artifacts remain outside Git; verify complete fixture hashes.
- Never equate capacity, payload accounting, RSS and resident GPU memory.
- Preserve sample origin IDs and missing-capability reasons. Do not sum GPU scopes.
- Browser worker profiles sample stack presence, not exact OS thread CPU time.
- A physical-mobile result requires an actual device connection and observations.
- Keep previous collector protocols/histories intact; update the migration checklist.

## Tasks

- [x] Extend `renderer/telemetry` and viewer performance telemetry with opt-in GPU
  sampling, stable source frame IDs and deduplicated buffer observations. Verify
  allocation aliases and retirement without retaining resource handles.
- [x] Add `tools/perf/src/bin/lod.rs` and supporting modules. Replay the prepared
  Playbot hierarchy through production packed rendering with projection caching,
  fixed static/moving/near-far-return cases, captures and completed frame samples.
- [x] Add browser worker profiling and physical-device connection helpers. Test
  a real headless worker, preserve cancellation and profile retention bounds,
  reject inconsistent device declarations and redact connection endpoints.
- [x] Extend browser collection and Python aggregation. Test delayed GPU arrivals,
  duplicate histories, invalid samples, sparse coverage, buffer totals, worker
  profiling isolation and native hierarchy fixture binding before integration.
- [ ] Run focused native tests/Clippy, GPU pixel checks, production WASM build and
  hardware native/browser replays without concurrent builds or measurements.
- [ ] Commit the clean producer, measure three independent processes/rounds,
  revalidate saved data and publish supported series to Bencher. Preserve prior
  plots and establish new collector histories rather than joining old timings.
- [ ] Record actual coverage, unsupported driver residency/exact worker CPU,
  device availability and evidence in the performance docs and checklist.
