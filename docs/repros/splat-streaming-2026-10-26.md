# Shared SOG streaming acceptance: 2026-10-26

## Candidate and ownership

Base revision: `99b5ededb6f04eccffc1aa9601d81e221e683459`, with uncommitted shared
streaming, renderer-adapter, diagnostics, resolver and hosted-viewer changes.
One production implementation now lives in `dimensify_splat`; provider policy,
presets, presentation and final camera ownership remain consumer responsibilities.
The canonical worker source is `crates/splat/src/streaming/runtime/`.

The standalone viewer uses the same controller for hosted SOG hierarchy JSON.
Local Gaussian PLY and bundled SOG retain their whole-file path. Hosted hierarchies
use the shared default packed backend, screen-space policy, 4M selection budget,
two concurrent requests and platform-specific cache defaults. This is functional
acceptance rather than an equal-quality performance comparison.

## Browser reproduction

Build/package/serve using the [viewer guide](../splat-viewer.md), including the
`worker` codec feature and both worker assets. Run `scripts/check-splat-browser.cjs`
against the served build with Chromium headless. The preserved run uses Chromium
151.0.7922.34, macOS WebGPU/Metal, a 1000 by 760 viewport and the synthetic eight-row
PLY/SOG and hosted hierarchy fixtures. Source is +Z-up initially. The camera starts
at the origin in Fly, loads preserve it, and the harness explicitly Frames before
viewport comparisons. No visible windows are opened.

The harness checks shared Feathers composition, system chooser/PLY, drop/SOG,
Frame, Fly/Reset, Fly/Orbit, pointer ownership, orbit/pan/zoom, signed cross-origin
URL/redirect/paste, hosted metadata and texture fetches, real codec-worker
completion, live signed-axis rotation/restoration and failure-preserving replacement.
Manifest query credentials are retained only on their own request. Invalid
hierarchy JSON and three failed initial chunk attempts leave the displayed cloud
intact. Unexpected browser errors fail acceptance; only the deliberately missing
chunk's 404 is expected.

Camera comparisons wait for the preceding gesture to settle and permit at most
one channel level of GPU rounding variation. Movement checks remain separate.
The inspected streamed and failure-preserving screenshots show the colored
fixture; the changed-axis screenshot visibly reorients it.

Evidence is local under `/tmp/dimensify-stream-browser-frozen-2026-10-26/`.
The matching artifact manifest is `/tmp/dimensify-stream-artifacts.json`.
Viewer WASM SHA-256:
`142d07d9331a9c83f4c5d77af5fd02d62d6c6aa5d9ae084d4ddcfd6a3af48956`.
Worker codec WASM SHA-256:
`1fc064d84c285ef8398b1f3df619cbe3a5512988c517df2f316847468ef3aae0`.
The worker is built with the release profile; the viewer uses the development
profile. A public-purpose documentation line was added after compilation without
changing runtime tokens. Generated packages are ignored and omitted from source
exports; source exports rebuild them using the guide.

## Supporting verification

- Shared viewer/controller suite: 242 passed, 41 optional GPU/large-scene cases
  ignored. Decoder integration: 11 passed, one optional captured-scene case ignored.
- Policy-only suite: 11 passed with no default features. Native streaming also
  checks without browser UI or a selected WASM backend feature.
- Real worker wire tests and asynchronous job/cancellation tests: 15 passed.
- Offscreen Metal on Apple M5 Max: decoded coarse/fine footprints preserve
  coverage across snapshot/resident modes and transformed views; packed refinement
  retains packed sources and releases retired source ownership. Both explicit
  replays pass. Other optional GPU tests were not run in this slice.
- Browser WASM builds and library Clippy pass, with migrated/style/deprecation
  warnings. This is not a claim that strict warning-free Clippy passes.
- Independent source export resolves local manifests entirely within its tree.
  Hub, camera/inspect and policy-only splat graphs exclude renderer, windowing,
  robot and physics dependencies.

Commands and logs are local under `/tmp/dimensify-stream-*.log`. The viewer guide
owns current commands; this record preserves the fixture, artifact and limits.
The updated [gap audit](../supersplat-gap-audit.md) distinguishes streamed residency
from whole-file input and the remaining format/editor/rendering gaps.

## Unverified scope

No new captured multi-million-row scene replay, other device/browser matrix,
native OS file interaction, release performance comparison or published-host
acceptance was performed. Standalone WebGL is still unsupported. Local folder
packages, flat unbundled metadata entrypoints, new formats, stochastic alpha,
Auto Dolly, shared quality panels and editor workflows are outside this slice.

## Follow-up quality observations

Base `c99a07d3`, with a shared `SplatDiagnosticsPlugin`, `SogQualityReport` and
bounded `SogLeafSnapshot`. This moves the existing quality/memory observation
implementation into the library; presets and presentation remain consumer-owned.
The standalone streaming plugin installs the observer without adding a window,
UI or synchronous GPU readback. Unchanged reports retain change-detection state.

Three standalone lifecycle/bounds/change-publication tests pass, together with
the existing 242 shared controller tests and 11 decoder tests. The corresponding
consumer suite passes 42 Gaussian behavior checks and five development-diagnostics
contract checks. Both consumer WASM backend checks pass. Library Clippy passes
with existing migrated warnings; independent source export and headless graphs
remain isolated. No new render mode, quality panel or performance claim is added.

The same 17-check headless browser replay passes after installing the observer.
Evidence is `/tmp/dimensify-quality-browser-2026-10-26/`; the inspected hosted
fixture retains its displayed cloud after injected failures. The development
viewer WASM SHA-256 is
`6bc766d1f3c825e0cd28bbf20f95aca253f2050a7c28d004574d3826592c63ed`.
The codec worker is unchanged. Source/artifact and runtime limits above still
apply; the new runtime checks do not establish release performance or device
parity. Logs are local under `/tmp/dimensify-quality-*.log`.
