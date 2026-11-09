# Point-cloud integration patches

Each item must be marked `upstream`, `replayed`, or `obsolete` during the Bevy
0.19 rebase, with the implementing source path or a concrete rationale.

- [x] `upstream` — Potree hierarchy and node loading use the rewritten async
  `OctreeLoader` API in `src/loader/potree/`; zero-length reads are represented
  as `bytes::Bytes` rather than rejected by the renderer.
- [x] `replayed` — direct HTTP(S) byte sources are enabled through the
  `http_source` and `potree` features in `Cargo.toml`.
  `src/loader/potree/asset.rs` preserves site-revision query/fragment suffixes
  after each Potree filename, and HTTP status errors identify the failed method
  and URL.
- [x] `replayed` — whole-cloud Potree bounds use valid position-attribute
  extrema in `src/loader/potree/mod.rs`, with the padded root cube as fallback.
  Camera and section fitting therefore follow the content; hierarchy-node
  bounds retain their original cubes for streaming and visibility.
- [x] `replayed` — `src/server/` retains upstream cancellation and node
  lifecycle cleanup, while `src/server/task.rs` deduplicates queued hierarchy
  and chunk requests independently. Visibility reprioritization clears both
  heaps and their membership sets each frame.
- [x] `upstream` — `src/visibility/` implements heap-based hierarchical
  selection and `PointCloudVisibilitySettings::point_budget`. Product-specific
  memory, point, and DPR caps remain configured by The consuming application.
- [x] `upstream` — the `trace`/`trace_tracy` features and visibility diagnostic
  measurements remain optional in `Cargo.toml` and `src/visibility/`.
- [x] `replayed` — X-ray/dollhouse clipping is folded into the existing
  per-cloud uniform (`src/components.rs`, `src/render/point_cloud.rs`) and
  evaluated by both standard/simple material shaders without adding a bind
  group. The consuming application transforms repository-space planes into Bevy world space.
- [x] `upstream` — WebGL2 specialization is separately gated by `webgl2` in
  `src/render/pipeline.rs`; The host selects the appropriate backend feature.
- [x] `upstream` — Bevy 0.19's mesh/material pipeline owns view and material
  groups and `src/render/pipeline.rs` keeps point-cloud data within the fourth
  group.
- [x] `replayed` — `src/render/extract.rs` explicitly attaches point-cloud
  visibility state to extracted camera views and directional lights. Bevy 0.19 caches its camera
  extraction bundle before this plugin can register extra required components,
  and intentionally defers `ExtractSchedule` commands. The immediate
  render-world initializer is scheduled after camera/light extraction and before
  point-cloud visibility extraction. Inactive cameras and hidden/non-shadow
  lights are skipped; first-frame deferred extraction is not a missing-light
  warning. Existing per-view/cascade state is retained.
- [x] `replayed` — `src/render/prepare.rs` derives the root uniform's splat
  vertex offset from the point-cloud splat mesh, which exists before streamed
  chunk meshes; this avoids dropping the uniform during initial streaming.
- [x] `replayed` — adaptive Potree sizing in
  `src/assets/shaders/pointcloud_functions.wgsl` projects spacing to pixels,
  clamps there, then converts back to view space. Applying spacing after the
  upstream pixel-normalization shrank meter-scale scans by roughly the viewport
  dimension.
- [x] `replayed` — the standard point-cloud shader follows bevy_pointcloud's
  existing convention of interpreting normalized vertex RGB as sRGB, converting
  it to linear before Bevy's PBR input. Potree metadata does not declare a color
  space; this preserves the legacy renderer's behaviour for existing datasets.
- [x] `replayed` — `src/materials/standard/mod.rs` adds `unlit` to the
  point-cloud material pipeline key alongside Bevy's unchanged material key.
  `pbr.wgsl` omits lighting imports/calls for that variant while retaining
  base color, clipping, alpha handling and post-lighting processing. Lit
  materials retain their existing path. This avoids compiling unused cube-array
  shadow comparisons on affected mobile Vulkan drivers; physical-device acceptance remains separate from shader compilation.
- [ ] `integration` — accumulation-format fallback belonged to the removed
  multi-pass renderer and must be re-evaluated against the new direct pipeline
  during browser verification.

Relevant repository commits include `b8919b3d`, `68467dc7`, `a18c6f4d`,
`3ce317bb`, `70193356`, `aa4e1016`, `b228c0a4`, `1838f7de`, `d384049e`,
`a6965d55`, `f3ef0be0`, `194cc615`, `11cb107f`, and `66392675`.

## Optional isolated benchmark status (2026-09-30)

Off-by-default `render_status` adds a bounded per-view/sample snapshot bridge.
`DrawPointCloudInstanced` records only after its real draw is encoded, restricted
to main color phases (`Opaque3d`, `AlphaMask3d`, `Transparent3d`); reused prepass
and shadow commands cannot acknowledge benchmark samples. Chunk deduplication,
exact submitted point counts and overflow fail-closed status let the isolated
harness verify a fully resident synthetic fixture. Submitted instances are not
the number of unoccluded pixel-visible points. No harness imports, shader,
material, draw range, quality or feature-off render behavior changes are made.

## Shared loader integration (2026-11-07)

- `src/server/` forwards initial metadata/hierarchy errors as
  `PointCloudLoadFailed`, including the asset identity, through the ECS message
  queue. Consumers decide how to present errors or retry requests.
- The library no longer forces Bevy dynamic linking. Native applications can
  opt into dynamic linking through their own development features.
- Native Potree sources open `octree.bin` only when metadata selects the
  single-file layout. Multi-file converter output needs only `octree/<node>.bin`;
  missing single-file data still fails during initial metadata loading.
- Removing `PointCloud3d` retires its visibility class while preserving other
  classes, preventing stale camera entries and repeated warnings after a failed
  replacement or retired load.

## Point-size semantics (2026-11-09)

- `src/assets/shaders/functions.wgsl` projects a requested screen-pixel size
  back into view space without first normalizing it by viewport dimensions and
  reapplying perspective. Screen-pixel sizes now remain constant as the camera
  moves. World, screen and adaptive paths share a projection conversion that also
  handles orthographic cameras. The enum documentation reflects this contract.
- The Dimensify viewer enables adaptive sizing only for valid source spacing;
  sources without spacing use manual sizing instead of the shader's unit-spacing
  fallback. No spacing estimator or additional source samples are implied.

## Local directory sources (2026-11-10)

- `PotreeAssetSource::from_directory` accepts a relative-path opener for local or
  virtual files. It retains lazy single-file range reads and multi-file node
  reads, enabling browser File/Blob handles without HTTP or full-folder copies.
  Existing native and HTTP source constructors retain their layout behavior.
