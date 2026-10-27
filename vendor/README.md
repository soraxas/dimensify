# Viewer dependencies

This directory contains the canonical maintained
[`bevy_gaussian_splatting`](https://github.com/mosure/bevy_gaussian_splatting)
renderer (8.0.2, MIT OR Apache-2.0), plus its `splat_sort` and `sog_codec`
helpers. All path dependencies remain inside the standalone Dimensify tree.

The renderer is based on the `soraxas/bevy_gaussian_splatting` fork at
`b2c8e78848a25f71cb68bc62bf62148e6ad9bf0a`, with generic Bevy 0.19,
compositing, rasterization, sorting and SOG fixes. Diagnostic globals and
optional fixture/benchmark environment variables use Dimensify names.
Renderer tools, fixtures, subrepo provenance and original licenses are retained.
The helper crates preserve their source and manifests.

The standalone splat example selects PLY and bundled SOG input, SH3, storage
buffers and radix sorting on native and browser WebGPU. Other consumers select
their own renderer features. The maintained renderer also supports CPU-sorted
WebGL; that backend is not yet a supported standalone splat-example configuration.

Generic patch ownership and boundaries are recorded in
[LOCAL_PATCHES.md](bevy_gaussian_splatting/LOCAL_PATCHES.md).

The canonical `bevy_editor_cam` motion engine also lives here, retaining its
Bevy 0.19 fork provenance, examples, tests and MIT/Apache-2.0 licenses.
`dimensify_camera/inspect` selects it without default extensions. Render-only
dependencies are enabled by the relevant optional extensions; camera controls
do not require a renderer. See its [patch ledger](bevy_editor_cam/LOCAL_PATCHES.md).
