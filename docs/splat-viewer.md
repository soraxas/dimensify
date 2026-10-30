# PLY and SOG viewer

Dimensify can view Gaussian PLY and bundled SOG v2 files locally, on native
platforms or in a WebGPU browser. No room server, Python runtime or physics
backend is required. The reusable `dimensify_splat` crate is optional. Native
and WASM use the same retained Bevy Feathers toolbar, drop zone and status view.

## Native

From the Dimensify workspace:

```sh
cargo run --locked -p dimensify --features splat --example splat_viewer
cargo run --locked -p dimensify --features splat --example splat_viewer -- scene.ply
cargo run --locked -p dimensify --features splat --example splat_viewer -- --y-up scene.sog
```

Use **Open File** to choose a `.ply` or `.sog`, or drop one file anywhere in the
viewer. The empty scene contains a visible drop zone. The **File up axis** button selects which signed axis is vertical (+X, -X, +Y, -Y, +Z or -Z).
It rotates the loaded cloud immediately and also applies to the next file. Loading a replacement keeps the current scene visible;
read/decode failures appear in the status bar.

The camera starts at **(0, 0, 0)** looking along renderer -Z in **Fly** mode.
Loading a file preserves the camera. Use **Frame** or **F** to fit the cloud;
**Reset** returns to the origin, restores the initial orientation and clears motion.
**Mode** switches between Fly and Orbit without moving the camera.

In Fly, **WASD** moves, **Q/E** moves down/up, left drag looks, right drag pans,
and scrolling moves forward/back. **Shift** speeds movement up and **Ctrl** slows
it down. In Orbit, left drag orbits, right or Shift-left drag pans and scrolling
zooms. Ctrl-wheel orbits and Shift-wheel pans; Ctrl wins when both are held.
Orbit drag modifiers latch until release. A press starting over the toolbar
cannot acquire the camera after leaving it; losing window focus cancels motion.

## Browser

Install the `wasm32-unknown-unknown` target and a `wasm-bindgen` CLI matching
`Cargo.lock`, then build from this workspace:

```sh
cargo build --locked -p dimensify --features splat --example splat_viewer --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir splat-web/pkg target/wasm32-unknown-unknown/debug/examples/splat_viewer.wasm
cargo build --locked -p sog_codec --features worker --release --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir splat-web/sog-codec target/wasm32-unknown-unknown/release/sog_codec.wasm
cp crates/splat/src/streaming/runtime/sog-decode-worker.mjs splat-web/sog-decode-worker.mjs
python3 -m http.server 8080 --directory splat-web
```

Open `http://localhost:8080` in a browser with WebGPU enabled. Public hosting
requires HTTPS. Serve the complete `splat-web` directory including generated
`pkg/` files. The page hosts the canvas and initializes WASM. The shared Feathers
Open File button uses the browser's system file picker; dropped files are read
from their browser `File` objects through a thin Rust adapter. No filesystem
paths or uploads to a server are needed. The same File up axis control reorients the visible cloud without reopening it.

!!! note "Supported inputs"
    PLY must contain Gaussian attributes: positions, three log scales, opacity
    logit, quaternion and DC spherical harmonics. Ordinary XYZ/RGB point-cloud
    PLY and compressed PLY dialects are not Gaussian inputs supported by this
    loader. Local SOG must be a bundled v2 archive; selecting a flat `meta.json`
    plus separate WebP images is not supported. Hosted LOD hierarchy JSON is
    supported through Open URL.

!!! note "Coordinates and limits"
    Source coordinates default to right-handed Z-up. Y-up is an explicit option.
    One root transform maps Z-up into the renderer's Y-up coordinates; source
    assets are never rewritten. There is no fixed Gaussian-count cap. The loader
    checks file consistency, addressable allocation size and the active GPU's
    storage-buffer capacity before allocating the cloud. Each input is limited
    to 256 MiB. Large files can still require substantial browser memory while
    decoding and allocating GPU buffers. PLY and bundled SOG load a whole cloud; hosted hierarchy manifests use shared LOD streaming.

## Quality and renderer settings

Open **Settings** in the toolbar. Native and browser use the same scrollable
panel; use a wheel or drag the panel background on a touch screen. Settings last
for this viewer session. **Reset settings** restores the platform defaults.

| Controls | Application |
| --- | --- |
| Sorted / Stochastic | Live. Stochastic is experimental, visibly grainy and has no spatial resolve or temporal accumulation. |
| LOD policy, target footprint, splat ceiling and visibility | Streamed SOG only. Screen-space selection requires frustum visibility; the footprint applies only to that policy. |
| Decoded-cache, GPU source and GPU selection ceilings; source concurrency | Streamed SOG only. The labels distinguish global and per-scene budgets. Device limits can lower the effective selection. |
| Advanced: backend and projection cache | Explicit **Apply and reload scene**. Projection caching is experimental and requires Packed. |
| Advanced: compaction and radix depth precision | Live on supported sorted GPU paths; CPU standard sorting is available for whole-file and Snapshot rendering. |

Choose advanced backend/cache settings, then press **Apply and reload scene**.
The current scene and camera remain while a hidden replacement prepares. Only a
renderer-ready replacement becomes active. A failed, superseded or timed-out
reload retains the old scene and settings. Reloads require memory headroom for
both generations; budgets are not raised automatically. With a whole-file import,
backend/cache choices configure the next streamed scene and do not change the
file's expanded representation.

The panel shows requested, effective, selected and displayed splats, refinement
activity, limiting reasons, SH information and scoped memory observations. These
are partial allocation estimates/counters, not total VRAM or process memory.
Disabled controls describe their applicable path. No mobile quality preset or
speed advantage is implied by an experimental option.

For custom hosts, attach `runtime::SogStreamRenderSettings` to a newly spawned
`SogStream` to select a backend, projection cache and compaction independently.
Set storage options before its first update; use a new stream entity to change
them. Streams without the component retain global settings. The viewer owns its
session controls and replacement transaction; other consumers need not install UI.
Construct storage options with `SogStreamRenderSettings::new(backend)` and
`.with_projection_cache(true)`. Read them with `.backend()` and
`.projection_cache()`; only `compaction` can be edited live. Do not replace a
live stream's configuration component to change storage: spawn a replacement
stream so its decoded representation and resident allocations agree.

Apply waits while a scene load is pending. New file/URL selections take priority
over a queued reload, and finishing an ordinary load preserves current live
quality settings. The CPU cache control is capped at 4095 MiB on wasm32 so its
byte count stays representable.

## Touch controls

One finger orbits in Orbit mode and looks around in Fly mode. Two fingers pan;
spreading/pinching also zooms or moves forward/back in Fly. Use the retained
Frame, Reset and mode buttons to navigate without a keyboard. Toolbar buttons
have 44-pixel height and wrap with the window.

Gestures belong to their initial viewport. A touch started on UI cannot become
a camera drag; finger replacement, a third finger, focus loss, cancellation or
Reset rebases/cancels motion. Synthetic mouse input during a touch is suppressed.
The shared tracker also supplies the consuming application's existing touch
adapter; application tap/navigation/joystick policy stays in its host.

The [example gallery](../splat-web/gallery/index.html) is served at `/gallery/`
by the same static directory. Its live card opens this viewer; other cards show
native or headless commands. The [build and hosting guide](ci.md) covers the
release gallery and GitHub Pages deployment.

## Remote files

Choose **Open URL**, enter a full `https://host/path/scene.ply` or `.sog` link, or a hosted `lod-meta.json`,
then press **Load URL** or **Enter**. **Escape** or **Cancel** closes the panel.
Camera shortcuts are suppressed while entering the URL. Native and browser
use the same retained Feathers panel, decoder and scene replacement path.

Links may include signed query parameters and HTTP redirects. The original URL
path must end in `.ply`, `.sog` or `.json`. JSON must be a SOG LOD hierarchy,
not flat chunk metadata or an ordinary webpage/share link. The hierarchy fetches
its referenced chunk metadata and WebP textures progressively. Local directory
packages are not supported. Downloads stay on the client. They retain the current
camera, use the currently selected file up axis, and keep the current cloud on
HTTP/decode failure. A stream remains hidden until its first generation is
renderer-ready. A newer file or URL selection supersedes older results.

Browser remote hosts must allow cross-origin Fetch (CORS). An HTTPS viewer needs
HTTPS resource links; HTTP is useful for local development. The viewer does not
proxy private resources or configure provider authentication. Native requests
verify TLS normally. The existing 256 MiB input limit is enforced while reading
response chunks, including when Content-Length is absent; the same GPU capacity
checks apply after download.

!!! note "Hosted hierarchy authorization"
    All referenced assets need CORS access. A signed query on the manifest does
    not authorize its child files and is not forwarded to them. Standalone hosts
    must make the hierarchy and children independently accessible. Applications
    can supply an `AssetResolver` callback that returns an authorized URL, owned
    bytes, or an error for each logical reference. Provider login, account policy
    and signing belong to the consuming application.

## Dependency boundary

`dimensify/splat` enables the example and `dimensify_splat/viewer`. The latter
adds file selection and presentation. Applications can use `dimensify_splat`
with its default `render` feature to compose their own controls. Disable default
features to use manifest validation and screen-space LOD targeting without
rendering dependencies. `streaming` adds the production controller, immutable source residency, bounded
transport/decode and render-ready generation ownership. `SogLodSettings` and
`SogStreamStatus` are neutral core state; consumers own presets and presentation.
`SplatStreamingPlugin` installs the runtime; mark the selected camera with
`StreamingCamera`, then spawn a parsed `SogStream` under one source basis root.
The controller runs after transform and camera propagation and before visibility.
Renderer and codec
source is self-contained in [`vendor/`](../vendor/README.md). Session, transport
and hub graphs do not enable the viewer or renderer.

### Quality observations without a product UI

`SplatStreamingPlugin` also installs `SplatDiagnosticsPlugin`. Read
`dimensify_splat::diagnostics::quality::SogQualityReport` for requested, admitted,
selected and displayed counts, activity, device/source limits, SH capability and
memory accounting. Counts distinguish submitted work from render-ready evidence;
GPU estimates and reservations are labelled separately from observed allocation.
Reports publish only when values change and never perform synchronous GPU readback.

`SogDiagnosticSettings.enabled` opts into bounded leaf evidence through
`SogLeafSnapshot`: at most four scene headers and 256 leaf records, with complete
inventory counts retained. Disabling it releases per-stream leaf evidence.
Consumers apply configuration before `SplatDiagnosticsSet::Configure` in `Update`
and read reports after `SplatDiagnosticsSet::Publish` in `PostUpdate`. The plugin
can also be installed by a custom streaming host without a window or UI.
Presets, account policy, warning composition and presentation stay with the host.

The current targets are native and browser WebGPU. WebGL2 is not a supported
configuration for this example.

## Verification

```sh
cargo test --locked -p dimensify_splat --features viewer --lib --tests
python3 scripts/verify-standalone.py
```

With the browser build served locally, Playwright and a Chromium installation
can exercise the real file chooser, PLY input, SOG drop, framing and corrupt-file
replacement without opening a visible window:

```sh
node scripts/check-splat-browser.cjs http://localhost:8080 /tmp/dimensify-splat-viewer
```

The same acceptance covers signed cross-origin URLs, hosted hierarchy/chunk/texture
fetches, real worker completion, live stream axes and failed replacement.

Set `PLAYWRIGHT_MODULE` and `CHROMIUM_EXECUTABLE` if these are installed outside
the default Node module/browser locations. The script saves screenshots of the
empty, PLY, SOG, URL entry and invalid-file states for visual inspection.
Set `DIMENSIFY_SPLAT_FILE` to include a local captured scene in browser acceptance;
the script also saves `local-scene.png`. Keep captured assets outside the repository.

## Shared camera core

`dimensify_camera` supplies pose interpolation, semantic view requests, authored
FOV conventions, bounds/fit math, reusable Fly/Walk response and optional
inspect motion. The local-file viewer forwards semantic Fly input or mouse/wheel
inspect input to the shared controllers and retains its final pose writer.
See the [camera guide](camera.md) for input and pose ownership. It does not install a competing camera
writer or require a splat renderer or physics engine. The file viewer retains
its input adapter and applies the shared pose to its camera.

A minimal bounds example runs without opening a window:

```sh
cargo run --locked -p dimensify_camera --example frame_bounds
cargo run --locked -p dimensify_camera --example fly_camera
cargo run --locked -p dimensify_camera --example walk_camera
cargo run --locked -p dimensify_camera --features inspect --example inspect_camera
cargo test --locked -p dimensify_camera -p dimensify_interpolation --features dimensify_camera/inspect
cargo test --locked -p dimensify_splat --no-default-features --lib
```

Physical-device touch/gamepad acceptance and interactive Walk remain pending. Hosted hierarchies now use the shared production
cache, worker decode, LOD selection and render-ready lifecycle; whole-file PLY
and bundled SOG retain their simpler load path.

The browser package includes `sog-decode-worker.mjs` and `sog-codec/` beside
`index.html`. Build and deploy these with the matching viewer artifact. Worker
unavailability permits the serial fallback; an available worker whose assets are
missing reports a load error. `?sog_decode=serial` selects serial loading for
diagnostics. Worker statistics use `window.__DIMENSIFY_SOG_CODEC__`.

After building the worker codec, its wire and job lifecycle checks run with:

```sh
SOG_CODEC_DIR="$PWD/splat-web/sog-codec" bun test crates/splat/src/streaming/runtime/codec_jobs.test.ts crates/splat/src/streaming/runtime/sog-decode-worker.test.ts crates/splat/src/streaming/runtime/codec_worker.test.ts
```

For settings/reload acceptance, build the opt-in instrumented viewer with
`--features splat-viewer-perf` and serve its complete package (including the
codec worker). Run:

```sh
mise exec -- node scripts/check-splat-settings.cjs http://localhost:8080 dist/settings-evidence
```

This uses real canvas clicks and scroll/touch input. Read-only `perf=1&perf_ui=1`
observations locate the retained controls; they do not trigger viewer actions.
It checks live quality controls, backend/cache reloads, failure recovery and
camera preservation. Screenshots and logs remain local artifacts. Narrow desktop
layout/touch emulation does not establish physical-mobile performance.
