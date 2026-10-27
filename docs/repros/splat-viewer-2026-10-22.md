# Splat viewer acceptance: 2026-10-22

## Candidate and inputs

Base revision: `93f589c6d165bc74e0ee0345ae33bd562a4dd6ce`, with the uncommitted
PLY/SOG viewer changes. Native and WASM artifacts were rebuilt after the final
scene framing and normalized Gaussian raster profile changes. The served
`splat-web/pkg` output was regenerated from that WASM artifact.

Inputs are the authored eight-Gaussian cube fixtures in
`crates/splat/tests/fixtures/tiny.ply` and `tiny.sog`. Both use the default Z-up
source selection, the initial orbit camera, automatic framing and the normalized
Gaussian raster profile. Viewport: 1000 by 760 pixels.

## Observed result

The headless acceptance script in the [viewer guide](../splat-viewer.md#verification)
passed with Chromium 153.0.8010.12 on macOS using WebGPU and the Metal ANGLE
backend. The initial empty drop zone is visible. Selecting PLY through the actual
browser file chooser renders the colored cube; dropping SOG replaces it with the
equivalent scene. Frame remains usable. A corrupt SOG reports an error while
preserving the previous scene. No browser console errors were observed.

Inspected PLY and SOG captures show Gaussian footprints fully within the viewport.
The invalid-file capture retains those splats and shows the decoding error.
Evidence is local to this machine under
`/tmp/dimensify-splat-viewer-2026-10-22-final/`: `empty.png`, `ply.png`, `sog.png`
and `invalid.png`. No visible capture windows were opened.

The default headless graphics configuration lost its GPU device before loading;
the acceptance harness therefore selects Metal on macOS. This run establishes
behavior for that configuration only.

## Supporting checks and limits

- All 13 decoder/lifecycle tests and strict Clippy passed.
- Native and WASM example builds, native CLI help and isolated export passed.
- Moon graph checks, 14 planner tests and focused repository hooks passed.
- Framing tests include Gaussian support beyond the position bounds; lifecycle
  tests cover failed replacement and stale completion.

Native OS file-picker and drag interaction, other browsers/devices and release
performance remain unverified. Supported formats,
size limits and build commands belong in the [viewer guide](../splat-viewer.md).

## Five-million-Gaussian input

A local bundled SOG with 5,000,000 Gaussians (66,614,388 compressed bytes) exposed
the initial fixed two-million cap. That cap is removed. PLY payload length and
SOG texture dimensions now validate declared counts before allocation; checked
arithmetic and the active device's storage-buffer capacity constrain buffers.
The input-file byte limit remains as documented in the viewer guide.

The regression test first failed on the old count cap, then passed by reaching
texture-capacity validation for an intentionally incomplete large input. All 15
focused tests and strict Clippy pass. The optional local-input test also decodes
the full five-million cloud successfully. Native and WASM builds pass.

With the rebuilt WASM artifact, the same headless browser script additionally
selected the local SOG through browser File input. Status reported all 5,000,000
Gaussians, with no console errors. The inspected `local-scene.png` in
`/tmp/dimensify-splat-viewer-5m-2026-10-22/` shows rendered scene content.
Automatic framing is conservative: the scene occupies a small central area,
with peripheral splats visible. This verifies loading and rendering; camera
placement for exploring this captured interior and performance are not assessed.
The existing PLY/SOG fixture and failed-replacement checks also passed.
