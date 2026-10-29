# Memory and render-pass measurements

The real-scene runners retain raw observations and publish supported resource
metrics to Bencher alongside timing. See [performance tracking](performance.md)
for run and upload commands. Flat-cloud collectors use native protocol v2. Packed LOD uses its own native
protocol v1; browser observations use v4. Their workload identities keep older
histories separate.

| Measurement | Boundary |
| --- | --- |
| Native RSS and virtual memory | Process checkpoints before input, after input/decode/setup and after each camera case |
| Native peak RSS | OS `wait4` maximum for the entire child process, including captures; bytes on macOS, converted from KiB on Linux |
| Native CPU user/system time | OS process-lifetime accounting, including waited-for helpers; not decode-only CPU time |
| Decoded buffers | Vector capacity times element size, including unused reserved storage |
| Tracked GPU buffers | Actual unique `Buffer::size` values for cloud, sort and radix buffers, deduplicated by buffer identity |
| Packed LOD GPU buffers | Actual owner buffer IDs/capacities, including resident source, working slots, compact projection and sort/radix storage; aliases counted once |
| Metal device resource counter | Optional `currentAllocatedSize`, only after a 1 MiB allocation probe verifies it observes the active wgpu device; includes more than tracked buffers, excludes a residency claim |
| Main WASM memory | Linear-memory capacity observed at browser checkpoints |
| Main JavaScript heap | Approximate V8 `performance.memory` used/total/limit where exposed |
| Worker WASM memory | Largest observed single worker linear-memory capacity; not the sum of concurrent workers |
| Worker sampled profiles | Optional V8 stack samples and elapsed attribution; separate diagnostic execution, including idle/GC/cancelled work |
| Worker wall time | Observed handler, decode, admission, round-trip, dispatch and response intervals, with sample counts |

Resource inspection runs outside native frame timing. Peak RSS covers the whole
process lifetime. Browser memory inspection runs during the replay and changes
its observation protocol. Each metric aggregates three independent native
processes or three browser rounds; browser peaks are maxima of observations,
not a continuous high-water measurement. Worker counters retain failures and
coverage, so cancelled or incomplete jobs cannot silently become complete totals.

!!! warning "These values overlap"
    Do not add RSS, decoded capacity, WASM capacity, JavaScript heap and GPU
    buffer bytes into a memory total. They describe different, overlapping
    boundaries, particularly on unified-memory hardware. Reserved worker bytes
    remain a scheduler estimate. Tracked GPU buffers exclude textures, staging,
    driver allocations and other untracked resources; they are not resident VRAM.

## GPU pass coverage

With `--gpu-timestamps`, the native runner records eligible radix sorting,
Gaussian raster, encoded compositing encode/decode and compact projection
passes separately. Every sample retains its originating frame, view, scene and
selection IDs. Multiple segments are summed only within one scope and frame.
They are never summed into a whole GPU frame time.

The flat-cloud runner retains pending, invalid, dropped and evicted sample counts. Bencher
receives pass percentiles only with complete valid frame coverage and positive
durations. Raw quantized zeros remain zero; unavailable work is omitted with a
reason. A flat-cloud workload does not exercise compact projection. Native
`--encoded-compositing` explicitly enables HDR internal rendering and transfer
passes as a separate workload; the default remains linear rendering.

The packed LOD runner exercises production `SogStream`, source admission,
working slots and compact projection caching using the external Playbot hierarchy.
Static, moving and near/far/return phases each retain completed frames, quality,
CPU/cache observations and nonblank captures. Local disk resolution excludes WAN
transfer. Pre-run hash verification warms the OS file cache; a fresh process
does not establish cold disk or network loading. Native pass percentiles still require complete valid coverage.

Browser v4 collects delayed timestamp histories with their originating IDs. Its
pass percentiles describe **observed samples**, with retained/completed, invalid,
dropped, pending and history-gap counts. A 100 ms observer can miss frames; these
are not complete frame GPU measurements or per-camera pass timings. Buffer peaks
are maxima of observations across cold/warm replays. Packed source payload remains
separate from actual buffer capacities; category capacities overlap.

## Worker profile diagnostics

`perf-scenes run --kind browser ... --worker-profile` pauses workers at startup
and delays parent termination long enough to flush bounded V8 profiles. Run this
separately from timing benchmarks. Upload using `--diagnostic-history`: only
profile counters and sampled elapsed attribution enter the `worker-profiles`
branch. Frame/loading results from this perturbed run are excluded.

The `sampled-profile-time` measure uses nanoseconds of sampled elapsed attribution,
including idle/GC. It is neither OS thread CPU time nor successful decode time.
Raw stacks, errors, unflushed tails and retention limits remain available for
analysis. Missing coverage omits sampled time.

## Physical devices

The browser harness accepts `--target device.json` with either
`DIMENSIFY_CDP_ENDPOINT` or `BROWSERSTACK_WS_ENDPOINT`. Target JSON has
`kind: physical-mobile`, `id`, `model`, `os` and `provider` fields. It preserves
the device context viewport and DPR, records observed UA/touch/screen properties,
and rejects inconsistent declarations. Device observations cannot prove hardware
attestation or identify thermal throttling.

A connected phone must be able to reach the served viewer and every fixture URL
(for USB Android, configure appropriate `adb reverse` ports). Use the direct
`perf-browser` command with reachable URLs and provenance from `web-build`.
Remote cold-cache/process isolation is unverified, so these runs remain diagnostic.
The current replay rejects physical CSS viewports below 1000x760 because its
toolbar/input coordinates use the desktop layout. Add adaptive targeting and
verify camera motion on the actual device before enabling phone-sized metrics.
Desktop mobile emulation does not establish mobile GPU or memory performance.

## Remaining measurement limits

Portable total GPU allocation/residency, whole-frame GPU duration, pure worker CPU
execution, isolated worker queue/transfer costs, worker JavaScript heap and
browser process RSS remain unavailable. Missing capabilities are null with a
reason, never invented zero measurements. Physical mobile hardware is still
required to characterize mobile GPU, memory pressure and thermal behavior.
The Metal counter is scoped to verified API resource allocation, not physical
residency. Other backends preserve an explicit unavailable reason. Physical
mobile measurements require a connected device; no phone result is implied by
adding connection support.

New instrumentation creates a new benchmark cohort. Compare commits using the
same collector protocol, settings and hardware; keep older timing histories.
