# Production viewer browser replay

Build a separate release viewer with `splat-viewer-perf`, run wasm-bindgen as in
`docs/ci.md`, and serve its host page and SOG worker artifacts. Ordinary viewer
builds exclude this feature. Instrumented builds retain upstream render diagnostic
systems; `?perf=1` additionally installs fixed-size frame histories, CPU stage
aggregates and bounded source-cache traces. These observations have overhead.

```sh
mise exec -- cargo build --locked --release -p dimensify --features splat-viewer-perf --example splat_viewer --target wasm32-unknown-unknown
mise exec -- wasm-bindgen --target web --out-dir /path/to/site/pkg target/wasm32-unknown-unknown/release/examples/splat_viewer.wasm
node scripts/perf-browser.cjs --url http://127.0.0.1:8080/index.html --file /path/to/scene.ply --commit <served-base-commit-40-hex> --source-state clean --build-id sha256:<served-wasm-digest> --hardware '<machine and GPU>' --out /tmp/viewer-replay
```

For hosted hierarchies replace `--file` with `--asset-url <manifest URL>
--asset-id sha256:<asset-set-identity>`. Asset identity must cover the hierarchy's
child resources, not only its manifest. Use `--source-state modified` for local
uncommitted builds; the commit then identifies the base, while the artifact digest
identifies what was served. The harness cannot attest user-supplied build metadata.
`PLAYWRIGHT_MODULE` and `CHROMIUM_EXECUTABLE` work as in the browser acceptance
script. `--publication-check` rejects software adapters; no results are uploaded.

The harness uses the actual canvas file chooser or URL panel and the production
loader/streaming renderer. It records initial loader readiness and the first
frame whose visible clouds acknowledge both sort and draw encoding. It then waits
for positive logical render-ready coverage, no pending/refining work, and a stable
count for one second. Capped quality can settle; full requested detail is not
implied. The reported settling onset excludes the one-second confirmation window.
Each replay includes static and moving main/near-side views, pan/return and framing.
`--replay path.json` accepts 1..32 named `hold`, `turn-return`, `pan-return`,
`near-side` or `frame` actions; `--duration-ms` sets each action's duration.
The input sequence is deterministic, while browser input delivery and camera
integration remain wall-clock dependent. Raw camera poses permit checking this.

Cold means a fresh browser process/context and application. OS/driver shader caches
are not flushed. Warm means a second replay of the already loaded scene in the
same application with its live source caches. Reset plus Frame establishes the
same main camera for both replays; Orbit mode makes the side gesture orbit the
scene, and wheel input then moves closer. It is not a second file load or a
claim about HTTP-cache-only loading. Warm load/first-frame metrics are unavailable. Cold refinement settling starts
at submission; warm replay settling starts at replay entry and includes gestures
and screenshot capture overhead. It is labeled replay-settle, not isolated
streaming execution time.
`--splat-budget`, `--cache-mib`, `--source-mib` and `--concurrency` set explicit
opt-in benchmark query settings, recorded from the effective resources. Packed
source admission currently reserves more than 4 MiB per unknown source before
headers arrive; a 1 MiB budget cannot bootstrap even a tiny packed hierarchy.
Bounded per-stream status reasons explain these admission constraints.
Fresh contexts do not by themselves reproduce low-memory source eviction; supply
a representative hierarchy and constrained viewer settings for that diagnosis.

`raw.json`, per-view PNGs, browser logs and `bmf.json` are written locally.
The output directory must be new or empty, preventing stale successful metrics
from surviving a failed rerun. Raw data
includes the active Bevy adapter fields (which browsers may redact), CDP browser
GPU device inventory, browser, CSS/physical resolution, base commit, source state,
artifact and asset identities, effective settings, camera poses, pending work,
logical ready counts, CPU stages, bounded cache events, cache payload accounting
and raw frame intervals. Cache byte accounting is not process RSS or total VRAM.
Histories retain 4096 intervals; scenarios that exceed the cap fail. RAF is
collected for each scenario before later scenarios can replace its history. Source event
history truncation is explicit in its lifetime/dropped counters. Publication
eligibility requires a clean release identity and an identified active integrated
or discrete GPU; privacy-redacted WebGPU identity remains ineligible even when
CDP lists a hardware GPU. Inventory alone cannot attest the active device.
Eligibility is not statistical approval or a baseline.

Intervals have three separate meanings: Bevy main-world frame cadence, render
schedule frames that acknowledged sort/draw command encoding, and browser RAF
cadence. None is GPU execution, completed-frame latency, display latency or FPS.
GPU timestamp and single-flight completion timing remain null with explicit reasons.
Unlike the reference renderer lab, the continuous viewer does not serialize each
frame through `onSubmittedWorkDone()`/fence readback. Screenshots provide review
artifacts; the harness makes no first-visible-pixel timing claim.

Median (averaging the middle pair for even sample counts) and nearest-rank
P95/P99 are computed per run, then the median of each run's
percentile is emitted per cold/warm mode and named scenario. Keep static and moving
scenario identities separate when comparing renderer changes. Matching inputs,
physical resolution, camera poses, quality/settings and hardware are necessary;
these replays do not establish cross-engine image parity. Raw times use
milliseconds; BMF latency values use nanoseconds.

Run `node --test scripts/test_perf_browser.cjs` for result/statistics contracts.
The bounded Rust frame history is tested in `viewer::performance::tests` with the
viewer feature enabled. Real browser replay is still required after compilation.
