# Memory and render-pass measurements

The real-scene runners retain raw observations and publish supported resource
metrics to Bencher alongside timing. See [performance tracking](performance.md)
for run and upload commands. New collectors use native protocol v2 and browser
observation protocol v3; their workload identities keep older histories separate.

| Measurement | Boundary |
| --- | --- |
| Native RSS and virtual memory | Process checkpoints before input, after input/decode/setup and after each camera case |
| Native peak RSS | OS `wait4` maximum for the entire child process, including captures; bytes on macOS, converted from KiB on Linux |
| Native CPU user/system time | OS process-lifetime accounting, including waited-for helpers; not decode-only CPU time |
| Decoded buffers | Vector capacity times element size, including unused reserved storage |
| Tracked GPU buffers | Actual unique `Buffer::size` values for cloud, sort and radix buffers, deduplicated by buffer identity |
| Main WASM memory | Linear-memory capacity observed at browser checkpoints |
| Main JavaScript heap | Approximate V8 `performance.memory` used/total/limit where exposed |
| Worker WASM memory | Largest observed single worker linear-memory capacity; not the sum of concurrent workers |
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

The runner retains pending, invalid, dropped and evicted sample counts. Bencher
receives pass percentiles only with complete valid frame coverage and positive
durations. Raw quantized zeros remain zero; unavailable work is omitted with a
reason. A flat-cloud workload does not exercise compact projection. Native
`--encoded-compositing` explicitly enables HDR internal rendering and transfer
passes as a separate workload; the default remains linear rendering.

## Remaining measurement limits

True total GPU allocation/residency, whole-frame GPU duration, pure worker CPU
execution, isolated worker queue/transfer costs, worker JavaScript heap and
browser process RSS remain unavailable. Missing capabilities are null with a
reason, never invented zero measurements. Physical mobile hardware is still
required to characterize mobile GPU, memory pressure and thermal behavior.
The projection collector is implemented, but these native flat-cloud fixtures
do not exercise packed/streaming projection. Browser LOD replays still lack GPU
timestamp and actual GPU-buffer inventory collection.

New instrumentation creates a new benchmark cohort. Compare commits using the
same collector protocol, settings and hardware; keep older timing histories.
