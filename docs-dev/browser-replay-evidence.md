# Browser replay verification (2026-10-28)

Release WASM with `splat-viewer-perf`, base commit `68ac55481d192f0888baddb06054443511daa919`, modified sources.
Served WASM identity: `sha256:6bdf5e2cdbe027f7f7783bcbd2bf79b26eda44cc18130aad71e08789a80330fc`.
Browser: `153.0.8010.12`; CSS and physical viewport 1000 x 760, DPR 1.
Host: Apple M5 Max / Darwin arm64. The active WebGPU adapter identity is
privacy-redacted; CDP lists an Apple M5 Max Metal device. These are diagnostic
smoke replays, with concurrent CPU builds, and are not publication baselines.

| Replay | Cold/warm cases | Logical ready count | Requested budget | Evidence |
| --- | --- | --- | --- | --- |
| Generated binary PLY | 7 + 7 | 10000 / 10000 | 4000000 | `/tmp/dimensify-browser-perf-final-10k` |
| Two-LOD hosted SOG hierarchy | 7 + 7 | 16384 / 16384 | 20000 | `/tmp/dimensify-browser-perf-final-stream-limited` |

Both runs completed actual acknowledged sort/draw encoding in every case,
recorded per-case app/render-encoding/RAF intervals, and wrote raw JSON, BMF
(nanosecond latency) and screenshots. Cold and warm main-camera poses match.
The stream used 8 MiB CPU/source budgets and concurrency 1, selected the
16384-splat coarse LOD and reported `candidates exceeded selection budget` with
no pending work. Decode attempts and cache events remained at 24 and 3 across
cold and warm replay; these are observed stage/event counters, not source counts.
The fixture has large overlapping splats and produces a white overdraw stress
image; it does not establish visual parity or representative scene quality.
The 10k PLY screenshot visibly shows the generated shell of small splats.

An additional full-refinement run requested 100000 and displayed 65536 from the
same hierarchy (`/tmp/dimensify-browser-perf-representative-stream`), with 50
decode attempts and 6 cache events, unchanged across cold/warm replay. That run
preceded the final per-case RAF and empty-output-directory guards; use the final
limited run for those contracts. A 1 MiB packed-budget failure is retained in
`/tmp/dimensify-browser-perf-stream-fixed`: the production reservation exceeds
4 MiB before unknown source headers, so no source could be admitted.

Verification: 19 JavaScript contracts passed; the native Rust bounded-history
test passed under the ci profile. WASM checks passed with both `splat` and
`splat-viewer-perf`; the release instrumented build passed. A real browser visit
with `perf=0` confirmed telemetry remained absent. GPU execution/completion and
first-visible-pixel latency are explicitly unavailable in this browser protocol.
