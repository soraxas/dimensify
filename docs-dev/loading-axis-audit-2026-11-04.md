# Local SOG loading and coordinate-convention audit

Measured on 2026-11-04: Apple M5 Max, 48 GiB unified memory, macOS Metal,
Chromium 153.0.8010.12. Dimensify revision `029b180`, optimized instrumented WASM
with SHA256 `c538a635e674cd2c2de3f1c98c1a832954418174be9517024fb2efa256b50f7c`.
The build manifest records base `f2ba0c4` plus the subsequently committed Feathers
patch. The reference is the existing bundled SuperSplat adapter from a local,
ignored checkout of [3dgs-renderer-bench at `7815826`](https://github.com/WilliamLiu-1997/3dgs-renderer-bench/tree/7815826c9e16b9f83c0a5a0aab5f4d3a21dcfd53/harness).
It is not a rebuild of the latest editor. External checkouts are not repository
fixtures or submodules; the evidence bundle records the modified adapter hash.

## Protocol and measurements

Three fresh Chromium processes per implementation/fixture, alternating order.
Hash-verified cached inlier-data bundled SOG files are served over localhost with
HTTP caching disabled. Physical viewport 1000x760, DPR 1, FOV 45 degrees,
eye `(4,3,6)`, target `(0,0,0)`, source converted with X rotation 180 degrees in
both implementations. All 12 runs load the expected splat counts without page
errors. Captures were inspected for nonblank scene geometry and matching camera
orientation; they are not quality-equivalence assertions. Apartment's camera is
inside/near geometry, so its capture is not a useful scene overview.

Medians in milliseconds:

| Fixture | Splats / source SH | Dimensify request to render observation | Reference request to first completed draw | Dimensify after response end | Reference after response end |
| --- | --- | --- | --- | --- | --- |
| Apartment | 661,466 / SH0 | 627.4 | 230.7 | 472.8 | 152.2 |
| Junk Shop | 1,000,000 / SH3 | 960.8 | 575.3 | 754.9 | 415.0 |

!!! warning "Different readiness endpoints"
    Dimensify's endpoint is the first published ready state with Gaussian draw
    encoding, observed on a 100 ms telemetry cadence. The reference waits for
    its first draw's submitted GPU work to complete. Neither measures display
    presentation. These are diagnostic loading observations, not a precise
    matched-endpoint speedup ratio or current SuperSplat editor parity result.

Dimensify main WASM capacity at readiness is a median **512.75 MiB** for the
7.99 MiB Apartment bundle and **742.31 MiB** for the 15.63 MiB Junk Shop bundle.
Capacity includes reserved memory and is not live allocations, peak RSS or GPU
memory. Apartment has SH0 input while Dimensify reserves the compiled SH3 layout;
the reference selects zero SH bands. This is a concrete layout difference,
not an experimentally isolated explanation of the complete timing gap.

Whole-file CPU-stage telemetry reports zero decode attempts despite successful
loads. It does not instrument this path. Decode, validation/bounds scans,
expansion, asset preparation and GPU upload cannot yet be timed separately from
these reports. Do not treat missing timings as zero work.

Local evidence is in ignored `dist/loading-comparison-20261104/`: `raw.json`,
`summary.json`, `fixtures.json`, `run.cjs`, `reference.html`, codec/bundle hashes
and per-run captures. Initial failed reference probes are retained separately;
they are excluded from final `raw.json`. The adapter's hardcoded `.ply` suffix
was removed in the ignored bundle copy so it reads the unchanged SOG fixtures.
Its missing WebP WASM decoder was restored from the pinned splat-transform 3.6.4
package. Models and captures are not committed or republished.

No PLY, streaming LOD, physical-mobile or frame-throughput conclusions follow
from this experiment. No Bencher regression result was published from this
ad-hoc, differently instrumented comparison.

## Why a SuperSplat asset may need -Y

PlayCanvas/SuperSplat world coordinates are right-handed and +Y up. Its loader
applies the source's transform; splat-transform's PLY convention (also used for
SOG) is a 180-degree Z rotation: `(x,y,z) -> (-x,-y,z)`. Source -Y therefore maps
to world +Y. This is a rotation with determinant +1, not a handedness reflection.
It is an import convention, not proof that every PLY or SOG asset is authored
with the same vertical direction.

Dimensify `SourceUp::NegativeY` instead rotates 180 degrees around X:
`(x,y,z) -> (x,-y,-z)`. Both produce an upright source -Y, but their world
headings differ by 180 degrees around Y. The existing unit test verifies all
signed up axes map to +Y with determinant +1; it does not assert SuperSplat
heading compatibility.

References:

- [SplatTransform Transform.PLY convention](https://api.playcanvas.com/splat-transform/one-page.html#ply)
- [SuperSplat world-coordinate contract](https://github.com/playcanvas/supersplat/blob/main/docs/webmcp.md)
- Source inspection used a separate ignored SuperSplat checkout at `b89029d`:
  [asset-loader.ts](https://github.com/playcanvas/supersplat/blob/b89029dd20911fd18905b61de2701094ac390a73/src/asset-loader.ts#L109)
  consumes `transform.rotation`; [splat.ts](https://github.com/playcanvas/supersplat/blob/b89029dd20911fd18905b61de2701094ac390a73/src/splat.ts#L122)
  applies it to the root entity. This source inspection is separate from the
  benchmark adapter's bundled version.

## Follow-up

- [x] Instrument whole-file reads, SOG decode, SH expansion, validation/bounds
  and main-world admission (see the follow-up implementation below).
- [ ] Instrument render-world preparation and GPU upload/completion separately.
- [ ] Add matched first-completed-draw endpoints and rebuild the current
  SuperSplat reference from pinned source before establishing a benchmark gate.
- [ ] Investigate retaining compact/SH0 data for whole-file imports. Validate
  visual output and matched loading measurements before claiming a gain.
- [x] Replace the signed-axis cycle with fixed coordinate-convention presets
  and explicit signed-up-axis choices. The SuperSplat import preset uses the full
  Z half-turn while existing `up=` links retain their original rotations.
- [ ] Support arbitrary authored orientations beyond the fixed presets and
  signed-axis mappings; no arbitrary rotation/quaternion input exists yet.


## Follow-up implementation and stage capture

The coordinate presets and opt-in whole-file timers were implemented on the same
base (`029b180` plus the loading/preset patch). The optimized viewer WASM SHA256
is `aecfd00bfc891e8a427df39658b3312a4e4b7d860891b1a7aecd2891fb1278cc`. Three fresh Chromium processes
per fixture on the same M5 Max/Metal returned the expected counts without page
errors, using `orientation=3dgs`, the same viewport and authored camera. Source
hashes were rechecked. This is a diagnostic capture, not a matched baseline/
candidate optimization experiment; the source orientation differs from the
previous X-half-turn protocol.

Median total elapsed milliseconds per process:

| Stage | Apartment SH0 | Junk Shop SH3 |
| --- | ---: | ---: |
| Input read/body copy | 158.1 | 202.2 |
| Input preflight | 4.1 | 6.0 |
| Archive read | 2.8 | 5.3 |
| Expanded-plane allocation | 10.3 | 15.0 |
| WebP decode | 60.6 | 121.1 |
| Attribute expansion | 40.1 | 202.8 |
| Plane transfer | 0.1 | 0.2 |
| Validation/bounds | 33.1 | 50.6 |
| Main-world scene admission | 18.5 | 27.8 |

The capture identifies measurable decode/expansion and validation work. It does
not isolate GPU upload/completion or prove which change would close the total
loading gap. The largest preparation stage for Junk Shop is attribute expansion;
SH0 layout residency remains a separate memory investigation. These wall times
are not thread CPU samples. Per-stage medians are not a total load measurement.

Evidence is retained in ignored `dist/loading-stages/`: raw observations, build
identity, source diff/new modules, fixture hashes, harness, captures, summary and
validated Bencher-format stage exports. Quantized zero intervals retain attempts
but omit positive-latency exports. No ad-hoc results were uploaded as a canonical
Bencher regression series. Standard real-scene browser reporting now includes
these observed stages automatically.

Verification: 284 native tests passed (45 GPU/platform cases explicitly ignored),
7 codec tests passed, native and WASM Clippy passed, and optimized WASM packaging
passed. Browser launch acceptance passed 19 checks; gallery acceptance passed at
both hosting layouts. Full chooser/drop/camera/hosted-stream acceptance passed.
Real settings input verified presets after deep scrolling,
pending/live stream orientation, camera preservation, backend replacement and
narrow touch ownership. Captures were inspected locally; no new Pixel Eagle
comparison was published by this task. Legacy positive-axis rendered captures
match their named presets within the existing unchanged tolerance.
