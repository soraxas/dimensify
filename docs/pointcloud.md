# Point-cloud loading and viewing

`dimensify_pointcloud` composes the vendored `bevy_pointcloud` renderer and Potree
reader. It is optional: headless transport/session consumers do not acquire its
rendering dependencies.

From the Dimensify repository root:

```sh
cargo run -p dimensify --features pointcloud --example pointcloud_fixture
cargo run -p dimensify --features pointcloud --example pointcloud_viewer -- --source /path/to/pcd --z-up
cargo run -p dimensify --features pointcloud --example pointcloud_viewer -- --source https://example.org/pcd/metadata.json
```

The fixture generates colored sphere vertices locally. Click **Open** to:

- **Open PLY file**: select an ordinary point-cloud PLY file.
- **Open Potree folder**: select the whole Potree 2 dataset folder, including
  `metadata.json`, `hierarchy.bin` and `octree.bin` or `octree/` node files.
- Paste an HTTP(S) PLY URL, Potree directory URL or `metadata.json` URL, then
  click **Load URL** or press Enter. Query suffixes are preserved.

You can also **drag a PLY file or a complete Potree folder onto the viewer**.
File and folder access works in native and browser viewers. Browser selections
stay on your device; no upload or local HTTP server is needed. Potree folders
retain browser file handles and read only requested ranges/nodes as you navigate.
Choose **Up axis: Y/Z** before opening a source. Loading frames the new cloud;
**Use built-in demo** restores the sphere. Esc closes the panel. Invalid
selections show an error without replacing the current source; download/decode
failures appear in the viewer status.

PLY supports ASCII, binary little-endian and binary big-endian, positions,
optional RGB/alpha and normals. Faces are ignored; the vertex element must
precede other nonempty elements. PLY loads as a whole file and has no Potree
hierarchy or spacing metadata, so adaptive sizing is unavailable. LAS/LAZ and
COPC import are not exposed by this viewer.

!!! note "HTTP hosting requirements"
    Remote sources require CORS when loaded across origins. PLY uses a whole-file
    GET. Potree single-file data uses byte ranges in `octree.bin`; multi-file
    data (`octreeLayout: "multi-file"`) uses ordinary GETs for
    `octree/<node>.bin`. Both Potree layouts read ranges from the shared
    `hierarchy.bin`. Local file/folder selections do not use HTTP.

Use `mise exec -- just showcase-catalog` to refresh discovery and
`mise exec -- just showcase-build --only pointcloud` to build the WebGPU viewer.
Open its gallery entry with `?source=<URL-encoded-source-URL>&up=z` for a Z-up
remote source. The gallery card's **Build a link** action opens
`gallery/build-link.html?viewer=pointcloud`. Choose a PLY/Potree source URL and
Y/Z up-axis to copy a viewer link or iframe. Point cloud startup supports only
`source` and `up`; camera options and toolbar hiding are unavailable.
Without a source it displays the fixture. Native `--source` also
accepts a PLY path, a Potree folder or `metadata.json` path. URL resolution and
asset authorization belong to the caller.

For a separate WebGL2 build:

```sh
cargo build -p dimensify --features pointcloud,dimensify_pointcloud/webgl --example pointcloud_viewer --target wasm32-unknown-unknown
```

Do not combine `webgl` and `webgpu` when selecting the browser backend.

## Live display controls

Use **Grid: Off** / **Grid: On** in either viewer to toggle an infinite ground
grid centered at the world origin on the Y=0 plane. It starts off and does not
move or frame the scene. The camera must be above or below the plane to see it.

Open **Settings** in the viewer to change point size and its minimum/maximum
pixel bounds, size units, adaptive sizing, shape and orientation. Changes apply
to the loaded cloud immediately. **Reset settings** restores the initial display
settings; the camera Reset action remains separate.

Shapes include a square billboard, a shader-clipped circular billboard and a
circle mesh. Orientation is independent: choose camera-facing or normal-aligned
points. Normal alignment uses source normals when present and the renderer's
fallback normal otherwise. Screen-size clamps are in pixels even when the base
point size is in world or local units.

Adaptive sizing uses source spacing and selected octree LOD instead of the base
point size and units. It is available only when the source supplies valid spacing
metadata. The sphere fixture has no spacing metadata, so its adaptive control is
disabled and it starts with 0.3 world-unit points and no pixel clamps.

Screen-pixel sizing keeps a fixed pixel footprint as the camera moves. World/local
sizing grows with zoom, like the distance between samples. Changing units converts
the manual size at the cloud centre to avoid reinterpreting a pixel value as world
units. Minimum/maximum bounds are always pixels; zero disables the corresponding
bound. A minimum can make distant points overlap, while a maximum can expose gaps
nearby. Adaptive sizing does not reconstruct a continuous surface or create samples
beyond the dataset's available detail.

Point budget and minimum node radius control Potree refinement; they do not
reduce the built-in fixture's vertex count. A higher node-radius threshold selects
coarser detail. Settings are local to the current viewer session.

!!! note
    A gallery card appears on the public site only after the complete gallery
    passes CI and Pages publishes its tested artifact. A merged commit alone
    does not update the hosted gallery.

## Inspect the actual viewer

```sh
just pointcloud-visual /tmp/pointcloud-viewer-images
```

The output directory must be new. This native GPU check renders the production
viewer at 1000x760 and 360x640 without creating visible windows. It captures
`ready`, `failed` and pending-HTTP `loading` states, checks visible text pixels,
glyph layout bounds, cloud presence and retirement, and sends keyboard events
for M, F and R. Inspect the PNGs under `desktop/` and `narrow/`; passing numeric
assertions does not replace visual review.

Each directory includes a `pointcloud-viewer` capture manifest accepted by the
Pixel Eagle reporter, observed adapter identity and source fingerprints.
Packaging neither uploads nor approves a baseline. Service reporting needs a
project token, an authoritative source-workflow sidecar and an explicit reviewed
baseline ID, or upload-only bootstrap. Gaussian reference runs are incompatible
with these viewer captures. See [visual testing](visual-testing.md#ci-and-pixel-eagle).
Browser rendering, file pickers, drops and HTTP requests are checked separately
with `mise exec -- just pointcloud-web-check <site-url> <evidence-directory>`.

## Embed the shared loader

```rust,ignore
use bevy::prelude::*;
use dimensify_pointcloud::{LoadPointcloud, PointcloudPlugin, PointcloudUp};

app.add_plugins(PointcloudPlugin::default());
app.world_mut().spawn(LoadPointcloud {
    up: PointcloudUp::Z,
    ..LoadPointcloud::potree("https://example.org/pcd/")
});
```

Install Bevy's rendering plugins first. The plugin creates no camera, UI or
application scene root. A host supplies its camera's
`renderer::PointCloudVisibilitySettings` and any cache limits through
`PointcloudPlugin::cache`. Low-level clipping, material and diagnostic APIs remain
available through `renderer`.

Read `PointcloudStatus` after `PointcloudSystems` in `PostUpdate`:

| State | Meaning |
| --- | --- |
| `Metadata` | Initial metadata/hierarchy is pending; the request deadline applies. |
| `Preparing` | Initial hierarchy is available; bounds may be available, but no nonempty mesh is prepared yet. |
| `Ready` | At least one nonempty chunk has a prepared mesh. |
| `Failed` | Source creation, initial loading or its deadline failed; `error` explains why. |

`active` and `queued` count this cloud's refinement work. They are not a fixed
percentage: moving the camera changes demand. `Ready` proves neither a presented
frame nor full residency. Bounds can be used for camera framing before `Ready`.
The renderer continues to retry failed refinement nodes using its existing policy.

Insert another `LoadPointcloud` on the same entity to replace it; previous render
chunks are retired while host-owned children remain. Despawning the entity retires
the request. Async results for a retired handle cannot update its replacement.

`PointcloudUp::Y` is identity and is appropriate when a parent already converts
the scene basis. `Z` rotates source coordinates and placement into Bevy's Y-up
basis once, before the parent transform.

For an ordinary PLY, use `PointcloudSource::PlyUrl`, `PlyPath` (native), or
`PlyFile { name, file: PointcloudFile::from_bytes(bytes) }`. For a selected virtual
directory, construct `PotreeFiles::new(relative_paths_and_files)` and use
`PointcloudSource::PotreeFiles`. These sources share the same status, retirement
and rendering path. `decode_ply(&bytes)` is available for hosts that need a mesh.
