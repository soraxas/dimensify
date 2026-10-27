# Shared Feathers viewer acceptance: 2026-10-23

## Candidate and reproduction

Base revision: `b180f8cb`, with the uncommitted shared Feathers UI changes.
Native and WASM example artifacts were rebuilt from this candidate; the browser
package was regenerated after replacing the deprecated non-send insertion API.

The [viewer guide](../splat-viewer.md) owns build and acceptance commands. The
headless browser script uses Chromium 153.0.8010.12 on macOS, WebGPU with Metal,
a 1000 by 760 viewport, the default camera and Z-up source selection. Inputs are
the authored eight-Gaussian PLY/SOG fixtures and a local bundled SOG containing
5,000,000 Gaussians (66,614,388 compressed bytes). Captured assets stay outside
the repository. The normalized Gaussian raster profile is unchanged.

## Observations

The browser page contains no HTML buttons or source selector. The common retained
Feathers toolbar, drop zone and status bar render inside the canvas. Clicking
Open File on that canvas invokes the real browser file chooser. PLY selection,
SOG drop, the Frame button and selection of the full five-million scene pass.
A corrupt SOG shows a visible error and preserves the previously rendered scene.
No browser console errors were observed.

The first synthetic click had no dwell between press and release and did not
activate the frame-driven widget. The harness uses a 120 ms press duration;
the real canvas file-chooser flow then passes. The ECS tests directly verify
Feathers activation emits a framing request and toggles the source frame and
its visible label.

Final evidence is local under `/tmp/dimensify-splat-feathers-2026-10-23-final/`:
`empty.png`, `ply.png`, `sog.png`, `local-scene.png`, `invalid.png` and
`browser.log`. Inspected fixture captures show the shared Feathers controls and
colored Gaussian cube. Inspected local-scene/error captures show the full-count
status followed by the visible error with the scene retained. Framing of the
captured interior remains conservative, as in the
[previous large-scene record](splat-viewer-2026-10-22.md#five-million-gaussian-input).
No visible capture windows were opened.

## Supporting checks and limits

All 17 focused tests, strict native Clippy, native/WASM example builds, native
CLI help, isolated export and focused repository hooks pass. The optional local
decoder test was not rerun separately; browser acceptance decodes and renders
that full input through the changed file adapter.

Native OS file-picker/drag interaction, other browsers/devices, mobile layout
and release performance remain unverified. The native UI activation tests run
without a window or GPU; browser runtime evidence does not establish native
window behavior.
