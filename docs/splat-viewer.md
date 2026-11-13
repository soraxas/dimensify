# Splat viewer

Dimensify can view Gaussian PLY and bundled SOG v2 files locally, on native
platforms or in a WebGPU browser. No room server, Python runtime or physics
backend is required. The reusable `dimensify_splat` crate is optional. Native
and WASM use the same retained Bevy Feathers toolbar, drop zone and status view.

The standalone viewer builds the 3D PLY/SOG renderer without the unused 4D
pipeline or glTF/GLB scene loader. Library consumers can opt into these vendor
registrations with the `dimensify_splat` features `gaussian-4d` and `gltf`.
These features do not extend the standalone viewer's file picker or URL parser.
SH3, GPU radix sorting, the CPU sort fallback, streaming and WebP decoding
remain available in the viewer.

## Native

From the Dimensify workspace:

```sh
cargo run --locked -p dimensify --features splat --example splat_viewer
cargo run --locked -p dimensify --features splat --example splat_viewer -- scene.ply
cargo run --locked -p dimensify --features splat --example splat_viewer -- --y-up scene.sog
```

Use **Open File** to choose a `.ply` or `.sog`, or drop one file anywhere in the
viewer. The empty scene contains a visible drop zone. Open **Settings > Coordinates** for import presets and six signed-axis choices.
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

### Pivot on splats

In Orbit mode, dragging and scrolling pivot on the splat under the pointer
rather than the focus plane. The viewer reads the rendered depth within a
12-pixel radius and uses the nearest surface. Sorted splats normally write no
depth, so on the frame a gesture starts they are drawn once more, depth only,
where a splat reaches the pivot opacity threshold. Colour output is unchanged and
no splat data is copied to the CPU. Stochastic rendering already writes depth.
Toggle pivoting with **Pivot on splats** in Settings > Rendering or the **O**
key; when it is off, or nothing is under the pointer, the camera pivots on its
focus plane.

**Pivot opacity threshold** (Settings > Rendering, 5–95%, default 25%) sets how
opaque a single splat must be to count as a surface. Raise it if pivots land on
floating haze in front of objects; lower it if faint surfaces are missed. Each
new value compiles a shader variant, so the first gesture after a change may
fall back to the focus plane. Hosts embedding the renderer set the same value
through `GaussianDepthPass::alpha` in the render world.
See [camera pivot picking](camera.md#pivot-picking-from-rendered-depth).

## Smooth camera movement

**Frame** / **F** eases from the visible pose to the fitted scene over 400 ms.
**Tour (P)** / **P** starts a six-second, 0.7-radian orbit and return around the
current focus. The tour eases to rest at its turn and returns to its exact starting
pose. Press P again to stop. Drag, touch, wheel or WASD/QE takes over from the
visible pose; focus loss also stops playback. Reset keeps its immediate behavior.
Existing orbit/fly input smoothing remains active for manual navigation.

Try loading a scene, press F, then P. Drag halfway through the tour to check
that taking control does not jump. Repeat at different frame rates: playback
samples absolute real time rather than accumulating rendered-frame movement.
Consumers can use `dimensify_camera::CameraPath` to sample the same motion without
installing the viewer or a renderer.

## Browser

Install the `wasm32-unknown-unknown` target and a `wasm-bindgen` CLI matching
`Cargo.lock`, then build from this workspace:

```sh
mise exec -- just splat-web-build
mise exec -- python3 -m http.server 8080 --directory splat-web
```

Open `http://localhost:8080/splat-viewer/` in a browser with WebGPU enabled. Public hosting
requires HTTPS. Serve the complete `splat-web` directory including generated
`pkg/` files. The page hosts the canvas and initializes WASM. The shared Feathers
Open File button uses the browser's system file picker; dropped files are read
from their browser `File` objects through a thin Rust adapter. No filesystem
paths or uploads to a server are needed. The same Settings > Coordinates control reorients the visible cloud without reopening it.

### Launch links and embedding

The `splat-viewer/` route accepts startup options on localhost and public hosting.
Use [Build a link](https://cs.tinyiu.com/dimensify/gallery/build-link.html)
to create a viewer URL and iframe snippet, or create one with `URLSearchParams`:

```js
const viewer = new URL("https://cs.tinyiu.com/dimensify/splat-viewer/");
viewer.search = new URLSearchParams({
  url: "https://assets.example.com/scene.sog?token=example",
  format: "auto",
  up: "z",
  eye: "4,3,6",
  target: "0,0,0",
  mode: "orbit",
  embed: "1",
});
document.querySelector("iframe").src = viewer.href;
```

| Option | Meaning |
| --- | --- |
| `url` | Direct HTTP(S) Gaussian `.ply`, bundled `.sog`, unbundled SOG `meta.json` or LOD hierarchy JSON. Relative URLs resolve against the viewer page. Signed query parameters survive encoding. |
| `format` | `auto` (default), `ply`, `sog` (bundled), `sog-unbundled` or `sog-stream` (LOD hierarchy). Explicit formats override filename extensions and permit extensionless endpoints. Auto inspects JSON content to distinguish SOG metadata from LOD. |
| `orientation` | `supersplat` (SuperSplat import; `3dgs` remains an alias), `y-up` (raw) or `z-up`. Cannot be combined with `up`. |
| `up` | Legacy source vertical axis: `x`, `y`, `z` (default), `-x`, `-y`, `-z`. |
| `eye`, `target` | Both required together, as `x,y,z`. Viewer-space positions after source-axis conversion; coordinates must be finite and within +/-10,000,000. |
| `fov` | Vertical field of view, 1 to 179 degrees; default 45. |
| `mode` | `fly` (default) or `orbit`. |
| `frame` | `1` fits once after loading; `0` preserves the initial camera. Scene links frame by default. Explicit `eye`/`target` take precedence. |
| `transparency` | `sorted` (default) or experimental `stochastic`. |
| `embed` | `1` hides the toolbar and drop overlay; loading/errors remain visible and clear on success. Camera input remains enabled. Default `0`. |

An authored camera also becomes the **Reset** home. These are startup options:
editing the URL with the History API does not reload a running viewer. Invalid or
duplicate known options show an error before starting a download.

For an unbundled dataset, set `url` to its `meta.json`; use
`format=sog-unbundled` to specify the format explicitly. The viewer fetches its
relative WebP files from the final metadata response URL (after redirects) and
uses the existing worker/packed-source pipeline. This
is one resolution, with no LOD refinement. The metadata's query parameters are
not copied to texture requests; every texture must be independently accessible
through the host's normal CORS policy. A mismatched explicit JSON format fails
before requesting textures.

!!! note "Renderer precision"
    Unbundled sources use the selected streaming backend (Packed by default).
    Its compact working set quantizes scale, rotation and color, so pixels can
    differ from the ordinary bundled-file renderer even for the same SOG data.
    Loader parity is checked against an identical source through the same backend.

```html
<iframe title="Dimensify splat viewer" width="100%" height="600"
  src="https://cs.tinyiu.com/dimensify/splat-viewer/?url=https%3A%2F%2Fassets.example.com%2Fscene.sog&amp;up=z&amp;embed=1"
  allow="fullscreen" allowfullscreen loading="lazy" referrerpolicy="no-referrer">
</iframe>
```

!!! note "Hosting"
    Both the page and iframe need a WebGPU-capable browser and HTTPS (or localhost).
    The asset host must allow CORS; embedding does not bypass it. The parent site's
    content security policy must allow this viewer origin in `frame-src`. A sandboxed
    iframe needs scripts and its normal origin. Links contain the complete asset URL,
    so review access tokens before sharing. The viewer sends no HTTP referrer.

Gallery scene presets reuse one viewer binary. Their 384-splat rings are generated
during the build; they illustrate orientation and transparency, not real-scene
loading or LOD performance. Real assets stay externally hosted or in a verified
local fixture cache. No generated models or reference images are committed.

After building and serving the site, verify real WebGPU launch behavior with
`mise exec -- just splat-web-check http://localhost:8080/`. It checks source-axis
output, embedded camera input, cross-origin signed SOG, unbundled/LOD source pixel
parity at the same packed backend, worker loading, extensionless format overrides, an iframe on another
origin, and rejection before dependent downloads for invalid settings.
Screenshots and logs go to `dist/splat-launch-check/`.

!!! note "Supported inputs"
    PLY must contain Gaussian attributes: positions, three log scales, opacity
    logit, quaternion and DC spherical harmonics. Ordinary XYZ/RGB point-cloud
    PLY and compressed PLY dialects are not Gaussian inputs supported by this
    loader. Local SOG must be a bundled v2 archive; selecting a flat `meta.json`
    plus separate WebP images is not supported by the file picker. Hosted
    unbundled SOG v2 and LOD hierarchy JSON are supported through Open URL and
    startup links. The URL format option applies to the startup URL.

!!! note "Coordinates and limits"
    Source coordinates default to right-handed Z-up. Y-up is an explicit option.
    One root transform maps Z-up into the renderer's Y-up coordinates; source
    assets are never rewritten. There is no fixed Gaussian-count cap. The loader
    checks file consistency, addressable allocation size and the active GPU's
    storage-buffer capacity before allocating the cloud. Local files and direct
    PLY/bundled SOG downloads have no fixed input-size cutoff. Large files can
    still require substantial browser memory while decoding and allocating GPU
    buffers. PLY and bundled SOG load a whole cloud;
    hosted unbundled SOG uses one complete source through the shared streaming
    machinery. Explicit `format=sog-unbundled` metadata is limited to 1 MiB while
    downloading. Auto JSON and explicit hierarchy metadata retain a 256 MiB
    transport limit; detected unbundled metadata is then validated
    against its 1 MiB limit before dependent downloads. Each texture is limited to 32 MiB.
    Whole-source CPU/GPU admission still applies. Hosted hierarchy manifests
    additionally provide view-dependent LOD refinement.

## Quality and renderer settings

The toolbar contains Open File, Open URL, Frame, Reset, navigation mode,
Tour and Settings. Coordinates and rendering mode live inside Settings.

Open **Settings** in the toolbar. Native and browser use the same scrollable
panel; use a wheel or drag the panel background on a touch screen. Settings last
for this viewer session. Panels, text and controls use the official Bevy Feathers
dark theme. Coordinates, Rendering, Level of detail (LOD), Memory and loading,
Advanced rendering and Diagnostics each expand/collapse independently. Rendering
and LOD start expanded. Sections expand/collapse with a reversible 200 ms height
animation; the browser skips animation when reduced motion is requested at
startup. Closing a section preserves its settings. Apply/reload notices remain visible outside
Diagnostics. Action buttons name the next mode or value. Boolean options use checkboxes; budgets and concurrency accept numeric
input as well as step buttons. Press Enter to commit an edit. Memory fields use
MiB, and values are clamped to the supported range. Camera shortcuts stay captured
while a numeric editor has focus. **Reset settings** restores live controls to the platform
defaults. For a loaded streamed scene, backend and projection-cache defaults are
staged; use **Apply and reload scene** to replace its storage configuration.

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

Choose **Open URL**, enter a full `https://host/path/scene.ply` or `.sog` link,
unbundled SOG `meta.json`, or a hosted `lod-meta.json`,
then press **Load URL** or **Enter**. **Escape** or **Cancel** closes the panel.
Camera shortcuts are suppressed while entering the URL. Native and browser
use the same retained Feathers panel, decoder and scene replacement path.

Links may include signed query parameters and HTTP redirects. The original URL
path must end in `.ply`, `.sog` or `.json` for automatic format detection.
JSON content distinguishes unbundled SOG v2 metadata from a SOG LOD hierarchy;
ordinary webpage/share links are unsupported. Use a startup link with an explicit
`format=ply`, `sog`, `sog-unbundled` or `sog-stream` for extensionless URLs or
other filename extensions. Open URL uses automatic format detection. Unbundled
metadata fetches its WebP textures; a hierarchy fetches referenced chunk metadata
and textures progressively. Local directory packages are not supported.
Downloads stay on the client. They retain the current camera, use the convention
selected in **Settings > Coordinates**, and keep the current cloud on
HTTP/decode failure. A stream remains hidden until its first generation is
renderer-ready. A newer file or URL selection supersedes older results.

Browser remote hosts must allow cross-origin Fetch (CORS). An HTTPS viewer needs
HTTPS resource links; HTTP is useful for local development. The viewer does not
proxy private resources or configure provider authentication. Native requests
verify TLS normally. Direct PLY and bundled SOG downloads have no fixed input-size
cutoff; allocation and GPU capacity checks still apply. Metadata limits are
enforced while reading response chunks, including when Content-Length is absent.

### Progressive PLY (opt-in)

To display binary little-endian Gaussian PLY as it downloads, start the native
viewer with `mise exec -- just splat-viewer --progressive-ply`, then use
**Open URL**. For the browser, add `progressive=1` to the viewer launch link:

```text
/splat-viewer/?url=https%3A%2F%2Fassets.example%2Fscene.ply&progressive=1
```

This option applies to remote PLY. Local files, bundled SOG and the default
remote PLY path keep whole-file loading. ASCII and big-endian PLY use the
whole-file decoder after download. There is no fixed input-size cutoff; platform
allocation and GPU capacity limits still apply.

The viewer decodes bounded batches and uploads only new rows. It reserves GPU
capacity from the header; unreceived rows stay invisible. Sorting uses the full
declared capacity throughout loading, so this option does not reduce final GPU
memory or guarantee lower frame times. It follows file order, which may reveal
parts of a scene rather than a coarse preview of the whole scene. Use an SOG LOD
hierarchy when camera-dependent refinement and residency are required.

The first batch appears after renderer readiness; the last successful scene is
retained for rollback until the new download validates completely. A failed or
truncated download restores that scene. New selections discard older pending
batches. Auto framing uses the first visible batch; **Frame** fits the current
bounds as additional data arrives. Browser fetch/decode yields between bounded
chunks, but decoding still runs on the browser thread in this first version.
Canceling a browser selection aborts its fetch. Native cancellation releases
queued/input data immediately; an outstanding blocking read can remain until
data arrives or the existing 120-second request timeout expires.

!!! note "Hosted hierarchy authorization"
    All referenced assets need CORS access. A signed query on the manifest does
    not authorize its child files and is not forwarded to them. Standalone hosts
    must make the hierarchy and children independently accessible. Applications
    can supply an `AssetResolver` callback that returns an authorized URL, owned
    bytes, or an error for each logical reference. Provider login, account policy
    and signing belong to the consuming application.

!!! note "Redirected scene names"
    Streaming metadata resolves relative resources against its final response URL;
    the displayed streamed-scene name currently uses that URL too. Binary PLY/SOG
    downloads retain the requested name.

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
configuration for this example. For what the streaming library's WebGL2 path
changes, see [browser backends](browser-backends.md#gaussian-splats).

## Verification

```sh
cargo test --locked -p dimensify_splat --features viewer --lib --tests
python3 scripts/verify-standalone.py
```

The full chooser/drop/settings acceptance needs an instrumented viewer because
it observes canvas control positions. Build the regular gallery first, then
overlay the instrumented viewer package (all generated files stay ignored):

```sh
mise exec -- just splat-web-build
mise exec -- python3 scripts/perf_scenes.py web-build --output dist/viewer-acceptance
mise exec -- python3 - <<'PY'
from pathlib import Path
from shutil import copytree, copyfile
from scripts.showcase import viewer_assets
source, site = Path("dist/viewer-acceptance"), Path("splat-web")
for name in ("pkg", "sog-codec"):
    copytree(source / name, site / name, dirs_exist_ok=True)
copyfile(source / "sog-decode-worker.mjs", site / "sog-decode-worker.mjs")
viewer_assets(site, site / "index.html")
PY
mise exec -- python3 -m http.server 8080 --directory splat-web
```

In another terminal, Playwright and Chromium can exercise file input, framing,
coordinate choices and corrupt-file replacement without a visible window:

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


## Source coordinate conventions

Open **Settings > Coordinates** to choose a preset:

| Choice | Source-to-viewer conversion |
| --- | --- |
| SuperSplat import (-Y up) | 180 degrees around Z: `(x,y,z) -> (-x,-y,z)` |
| Y-up (raw) | Identity; the file already uses viewer Y-up coordinates |
| Z-up | -90 degrees around X; preserves the existing default |
| Custom source up | The six existing signed-axis mappings |

The SuperSplat import preset makes source -Y vertical while retaining source +Z as +Z.
Custom -Y instead rotates around X and reverses Z. Both preserve handedness;
they have different headings. Files may use arbitrary authored coordinates, so
choose the convention that matches the source rather than assuming a format
always determines its orientation. Changing this setting reorients the current
and pending scene without changing the camera or downloading the file again.

Use `?orientation=supersplat`, `?orientation=y-up` or `?orientation=z-up` in viewer
links. Existing `?up=-y` and the other signed-axis links retain their exact
behavior. Supplying both `orientation` and `up` is rejected as ambiguous.
The link builder offers the same presets and an explicit custom-axis choice.
Native startup also accepts `--orientation supersplat` (or `y-up` / `z-up`).


The coordinate panel displays the selected source-up direction and all three
source-to-viewer axis mappings. For example, the SuperSplat import preset maps source -Y to
viewer +Y while preserving source +Z. This describes an import transform, not a
rendering algorithm or file format.

!!! note "Handedness is separate from up"
    All current presets use rotations, so they preserve handedness and do not
    mirror a scene. The viewer is right-handed, with +Y up and a camera looking
    along its local -Z. PLY/SOG input does not reliably declare the source frame;
    choosing an up direction does not detect its handedness or establish its
    forward axis. The panel reports the applied mapping rather than guessing a
    file's coordinate system. Converting a known left-handed source would need
    a separate reflection policy, including covariance/rotation and SH handling.

Existing `orientation=3dgs` links and native flags remain supported aliases for
`supersplat`. 3D Gaussian Splatting itself does not prescribe this import frame.


Check disclosure animation, reversal, reduced motion and toolbar composition:

```sh
mise exec -- just splat-disclosures-check http://localhost:8080/splat-viewer/ dist/disclosure-evidence
```

This uses the same instrumented viewer as settings acceptance. It checks computed
section heights during real canvas interaction; screenshots and samples remain
in the selected evidence directory. Samples record browser frame cadence and
observed app updates. A gap at or above the 200 ms animation can hide intermediate
heights; that run fails to establish animation coverage and must be investigated
or rerun on a responsive renderer.

## Camera playback diagnostics

Serve an optimized viewer built with `splat-viewer-perf`, as described in
[the browser replay guide](../docs-dev/browser-replay.md), then run:

```sh
mise exec -- just perf-camera-path --url http://127.0.0.1:8080/ \
  --file /path/to/scene.sog --out dist/camera-tour --rounds 3
```

Add `--headed` to watch Chromium. Use a separate `--capture` run for midpoint
images; capture runs discard cadence metrics so screenshots do not contaminate
timing. The diagnostic checks intermediate camera poses
against an independent analytic orbit, verifies exact return and pointer
cancellation, and saves raw telemetry, endpoint screenshots and median/P95/P99
app and encoded-render intervals. Position tolerance accounts for f32 coordinate
precision and is recorded in the artifact. Measurements cover warmed tours in one
session; they are neither cold-loading measurements nor GPU completion/display
FPS. They are diagnostic and never published to Bencher. Screenshots belong in
artifacts, with reviewed visual references in Pixel Eagle.
