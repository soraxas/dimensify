# Browser backends: WebGPU and WebGL2

Browser builds target either WebGPU or WebGL2. The backend is a compile-time
feature, so each build uses exactly one. WebGPU is the primary target: the
published gallery, the standalone viewers and all browser acceptance tests use
it. WebGL2 is a reduced fallback for consumers that need older browsers.

!!! warning "Select one backend per build"
    Enable either the `webgpu` or the `webgl` feature of a crate, never both.
    WebGL2 code paths are gated on `webgl` *without* `webgpu`, so combining them
    silently selects WebGPU behaviour.

## Support at a glance

| Component | WebGPU | WebGL2 |
| --- | --- | --- |
| Splat streaming library | All renderer backends | CPU snapshot renderer only, with reduced capacity and quality ([details](#gaussian-splats)) |
| Standalone splat viewer | Supported | Not supported: the `viewer` feature selects WebGPU |
| Point-cloud viewer and fixture | Supported, in the gallery | Builds; not in the gallery or browser tests |
| Point-cloud pivot picking | Supported | Falls back to the focus plane |
| Robot viewer and playground | Supported | Not supported: these features enable WebGPU |

The underlying reason is that WebGL2 has no compute shaders, storage buffers or
indirect draws. Features built on those resources either fall back or are
unavailable.

## Gaussian splats

The `dimensify_splat` crate's `webgl` feature installs the vendored renderer's
WebGL2 plugin in place of the storage and compute pipeline.

| Aspect | WebGPU | WebGL2 |
| --- | --- | --- |
| Renderer backend | `packed`, `gpu` (resident) or `snapshot`, set with `sog_renderer` | Always `snapshot`; other values warn and are ignored |
| SOG compaction and projection cache | Available | Disabled |
| Sorting | GPU radix sort or CPU sort | Depth sort in a web worker |
| Transparency | Sorted blending or stochastic transparency | Sorted blending only |
| Colour | Up to the build's spherical-harmonics degree (SH3 in the viewer) | Base colour only (SH0); no view-dependent colour |
| Splat storage | Storage buffers | 2D textures, four texels per splat, 512 splats per row |
| Default source cache | 256 MiB in the browser (512 MiB native) | 96 MiB |

WebGL2 capacity follows the device's maximum texture size: up to
`(min(size, 2048) / 4) × size` splats, about one million at 2048 pixels. The
renderer keeps two selected generations alive, and each view adds an index
stream, so a second camera roughly halves capacity under a tight budget. The
quality report names "Texture / index limits" when that, rather than the source
cache, constrains detail.

## Point clouds

The point-cloud renderer uses ordinary vertex and fragment shaders, so clouds
render on both backends. Under `webgl` its pipelines are specialized for
Bevy's WebGL2 bindings, and X-ray clipping planes live in the existing
per-cloud uniform to stay within WebGL2's bind-group limits.

The one feature missing on WebGL2 is [pivot picking](camera.md#pivot-picking-from-rendered-depth).
It reads the rendered depth through a compute pass, which WebGL2 cannot run, so
every pick misses immediately and the camera pivots on its focus plane. The
**Pivot on points** setting has no effect there. Multisampled (MSAA) depth
falls back the same way on any backend.

Build each backend separately:

```sh
# WebGPU (the gallery build)
mise exec -- just showcase-build --profile release --group pointcloud

# WebGL2
cargo build -p dimensify --features pointcloud,dimensify_pointcloud/webgl \
  --example pointcloud_viewer --target wasm32-unknown-unknown
```

## Verification coverage

| Check | WebGPU | WebGL2 |
| --- | --- | --- |
| WASM Clippy in CI | Yes | Point-cloud viewer and fixture only |
| Gallery build and publication | Yes | No |
| Browser acceptance suites | Yes | No |

!!! note
    WebGL2 builds are checked to compile, not to render correctly. The capacity
    rules above are covered by unit tests, but WebGL2 rendering, performance and
    the splat worker sort have not been verified in a browser. Treat WebGL2 as
    best effort until an acceptance suite runs against it.
