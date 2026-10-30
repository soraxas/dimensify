# SuperSplat capability gap audit

Source audit dated 2026-10-23. SuperSplat is a complete splat editor; the current
Dimensify example is a basic splat viewer. The difference includes data layout,
camera behavior, rendering policy and document operations, as well as UI.

Comparison revisions:

- Dimensify: `eeb8cac` (shared native/WASM Feathers UI).
- SuperSplat: `ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53`, package version 3.6.0,
  with PlayCanvas 2.23.3 and splat-transform 3.10.2. Its inspected checkout was clean.

This is a source inventory, not a comparative performance or visual benchmark.
SuperSplat links below are pinned to that revision. Dimensify status refers to
the active `splat_viewer` example, not every facility in Bevy or vendored code.
An example gap does not establish that another consumer lacks the feature.
Audit existing reusable consumers before implementing replacements.

**Present** means wired into the example. **Partial** means a narrower workflow.
**Available** means supporting APIs exist but need integration and validation.
**Missing** means the example needs a new capability. A field or shader file
alone does not establish a supported feature.

## Foundation extraction status

The shared foundation now owns the production SOG controller, immutable source
residency, cache/budget admission, screen-space selection/hysteresis, bounded
requests, browser worker decode and renderer-ready activation/retirement in
`dimensify_splat`. Applications retain provider authorization, presets and
presentation. One maintained renderer/codec/sorter remains under `vendor/`.

The example consumes the shared camera core for Fly and Orbit, starts at the
origin, preserves its camera on load, and offers Reset, explicit Frame and all
six signed source-up axes. Hosted hierarchy JSON loads through Open URL and the
shared streaming engine. Local PLY and bundled SOG keep the whole-file path.
Walk and its neutral environment interface have headless coverage; interactive
Walk, touch/gamepad, view cube and shared quality panels remain pending.

The [streaming extraction reproduction](repros/splat-streaming-2026-10-26.md)
records focused tests, offscreen fixtures and browser acceptance. These do not
establish full captured-scene, device or performance parity. Formats, stochastic
rendering, FOV Auto Dolly and editor/document operations retain their gaps below.

## Input and loading

SuperSplat's [file handler][ss-files], [lazy loader][ss-loader] and
[asset loader][ss-assets] define the import behavior. Dimensify's
[decoder](../crates/splat/src/decode.rs), [scene lifecycle](../crates/splat/src/scene.rs)
and [file adapter](../crates/splat/src/viewer/files.rs) define ours.

| Capability | Dimensify | Gap / distinction |
| --- | --- | --- |
| Standard Gaussian PLY | Present | ASCII and binary little-endian fixtures pass; required Gaussian attributes must be float32. Other encodings/dialects have no acceptance coverage. |
| PLY SH0, SH1, SH2, SH3 | Present | Contiguous coefficients supported; ordinary XYZ/RGB point-cloud PLY is rejected. |
| Compressed PLY | Missing | SuperSplat routes it through splat-transform. Our standard PLY parser is insufficient. |
| Bundled SOG | Partial | Bundled v2 supported. This is not a promise to accept every SOG version. |
| Unbundled SOG (`meta.json` + WebP) | Missing | Needs a related-file resolver, folder/multiple-file selection and metadata entrypoint. |
| Multi-LOD SOG (`lod-meta.json`) | Partial | Hosted hierarchies stream through Open URL; local directory packages and an import-level chooser remain absent. |
| `.splat` | Missing | No active decoder or picker support. |
| `.ksplat` | Missing | No active decoder or picker support. |
| `.spz` | Missing | No active decoder or picker support. |
| LCC / LCC2 | Missing | SuperSplat recognizes the metadata and sibling files; ours has no resolver or decoder. |
| Numbered PLY sequences | Missing | SuperSplat detects matching frame names and connects them to sequence playback. |
| Saved editor projects (`.ssproj`) | Missing | Requires a document model; this is not just another cloud decoder. |
| Imported camera poses | Missing | SuperSplat accepts INRIA camera JSON and COLMAP `images.txt`, including available intrinsics/FOV. |
| System file chooser | Present | Shared Feathers action; native and browser file-access adapters. |
| Drag and drop | Partial | One cloud replaces the current cloud. SuperSplat handles multiple files and folders, including related-file formats. |
| Multiple independent imports | Missing | Dimensify intentionally retains one current cloud. |
| URL loading / startup links | Partial | Open URL accepts signed HTTP(S) PLY, bundled SOG and hierarchy JSON. Startup links and repeated imports remain absent. |
| Native startup file | Present | CLI path and explicit `--y-up`. |
| OS/PWA file association | Missing | SuperSplat uses browser `launchQueue` where available. |
| Open/import recent | Missing | SuperSplat stores file handles; ours has no recent-file persistence. |
| Loading stages and byte progress | Partial | Ours reports reading/decoding/ready/error text; SuperSplat reports read progress and sort/upload stages. |
| LOD chooser before loading | Missing | SuperSplat suggests a single level using a 20-million preference threshold. That threshold is not a universal file rejection limit. |
| Lazy source reads and chunked upload preparation | Partial | Hierarchies fetch bounded chunks, decode in workers and activate renderer-ready generations; whole-file inputs still decode completely. |
| Invalid replacement preserves current cloud | Present | Tested; latest request wins and stale scene completions are rejected. |
| Input/device validation | Present | Finite geometry, count/payload consistency, 256 MiB input, ZIP expansion and GPU storage limits. No fixed Gaussian-count cap. |
| Continuous view-dependent LOD streaming | Present | Hosted hierarchies use shared screen-space selection and renderer-ready replacement. SuperSplat editor import-level selection is a different workflow. |

!!! note "Format names are not interchangeable"
    PLY is a container, not a guarantee of Gaussian attributes. Compressed PLY,
    standard Gaussian PLY and ordinary point-cloud PLY need different handling.
    Similarly, bundled SOG, unbundled SOG and multi-LOD SOG require different
    file-resolution and residency behavior. A WebP file is a SOG companion,
    not an independently supported Gaussian scene.

## Rendering, quality and memory

SuperSplat uses a [projected GPU renderer][ss-projected],
[packed source resources][ss-resource], [render-mode policy][ss-scene] and
[stochastic shader][ss-shader]. Dimensify wires
[CloudSettings](../vendor/bevy_gaussian_splatting/src/gaussian/settings.rs) through
[scene creation](../crates/splat/src/scene.rs), with renderer features selected in
the [crate manifest](../crates/splat/Cargo.toml).

| Capability | Dimensify | Gap / distinction |
| --- | --- | --- |
| Anisotropic Gaussian rendering | Present | Authored scales, rotations, opacity and SH are decoded; normalized Gaussian raster profile selected. |
| Sorted alpha blending | Present | GPU radix sorting selected by default. This does not establish matching color/blending behavior between engines. |
| Stochastic alpha | Experimental | Opt-in single-sample coverage and depth writes on the 3D storage/radix path. No spatial resolve or temporal accumulation; see [stochastic rendering](gaussian-stochastic-rendering.md). |
| Stochastic Disabled / Enabled / Movement / Auto | Partial | Explicit Sorted / Stochastic modes exist. Movement and Auto quality/motion policies remain missing. |
| Clean sorted frame after movement | Missing | SuperSplat's adaptive modes owe one sorted resolve frame after interaction settles. |
| Automatic expensive-frame detection | Missing | SuperSplat Auto uses sorted-frame GPU timing, with a 60 ms source default; without timestamp support it falls back to movement mode. |
| Runtime SH band selector | Missing | Our loader accepts several source degrees, but the renderer is compiled for SH3. SuperSplat selects runtime shader variants for bands 0-3. |
| Tone mapping selector | Available | Bevy offers tone mapping; example hardcodes `Tonemapping::None`. SuperSplat offers linear, neutral, ACES, ACES2, filmic and Hejl. Appearance must be verified, not inferred from option names. |
| Blend/color-space policy | Available | Vendor has opt-in encoded-space compositing and feedback. Example does not request it. Compare fixed-camera captures before choosing a default. |
| Global opacity and splat scale | Available | `CloudSettings` exposes both; no viewer controls. This is separate from editing authored per-Gaussian values. |
| Tint / temperature / saturation / brightness | Missing | SuperSplat has preview/apply/reset color grading, including selected Gaussians. |
| Black point / white point / transparency grading | Missing | Needs grading state, shaders, application semantics and export behavior. Global opacity alone is not equivalent. |
| Background and selection colors | Partial | Ours has a fixed background; SuperSplat has configurable, persisted appearance colors. |
| Centers, rings and selection outlines | Missing | No active viewer inspection modes or Gaussian selection. |
| Overdraw heatmap | Missing | SuperSplat provides a diagnostic render mode and legend. |
| Packed geometry and SH residency | Partial | Shared streaming supports snapshot, resident and packed backends with offscreen lifecycle coverage. Standalone streaming uses packed residency by default; no backend/quality panel. Whole-file imports remain expanded. |
| Projection cache / compact draw data | Partial | Shared streamed renderer adapters can compose resident/packed sources. Standalone has no controls for projection-cache or backend experiments; whole-file imports remain separate. |
| Visible-center compaction | Available | Vendor has opt-in `visibility_compaction`, default false. Not enabled by example. |
| Motion contribution / previous-depth occlusion culling | Missing | SuperSplat has these policies on its stochastic path. Vendor compaction is not evidence of equivalent culling. |
| Minimum projected-size culling | Missing | SuperSplat's editor command state defaults to 2 pixels; the inspected preferences panel does not expose a slider for it. Our example has no corresponding size-quality policy. |
| Antialiasing / fine-detail parity | Unverified | Both startup configurations disable ordinary MSAA. This says nothing about matching Gaussian footprint filtering, stochastic resolve, edge quality or fine-detail retention. Compare rendered results separately. |
| User-facing performance overlay | Missing | No FPS/frame-time/GPU/cull/residency display in our example. |
| Idle redraw control | Missing | SuperSplat disables automatic rendering and requests changed/settled frames. Example uses default window update behavior. |
| WebGPU browser target | Present | Both inspected editors require WebGPU. No WebGL compatibility advantage should be inferred for this SuperSplat revision. |
| WebGL2 fallback | Available | Vendor contains a separate CPU-sorted WebGL path; standalone viewer does not configure or validate it. |
| Native executable | Present | Shared Rust renderer and Feathers UI; useful independently of browser parity. |

### What stochastic alpha actually buys

The inspected SuperSplat path uses one stochastic coverage sample per pixel:
keep a fragment with probability related to alpha, write it opaque, and let the
depth test choose the visible survivor. It skips the per-frame sort in that
mode. The visible grain in the supplied screenshot is consistent with that
sampling tradeoff; it is not evidence of a bad file decoder.

Its source uses spatially stratified coverage and a resolve pass. Do not call
this temporal accumulation or assume it becomes noise-free after waiting.
Movement/Auto modes regain a clean image by returning to sorted blending after
movement. Enabled mode can remain stochastic at rest.

Our vendor's encoded compositing, additive blending and temporal sort shader
are different mechanisms. The experimental stochastic mode is a separate coverage/depth pipeline; it does not yet reproduce SuperSplat's spatial resolve or adaptive policies.

### Why five million Gaussians matter

Our SH3 representation reserves 48 float32 coefficients (192 bytes) per padded
Gaussian, even for lower-degree input. Five million rows therefore require
about **916 MiB for the SH plane alone**, before positions, rotations, scale,
opacity, sort buffers, CPU copies and render targets. That is a layout estimate,
not a measured peak-memory result. The decoder checks the GPU's per-buffer limit,
so successful loading on one GPU does not imply the same file fits every browser.

Removing an arbitrary count cap fixed the rejected-file symptom. Shared streamed
packed residency does not change the whole-file import layout; loading
responsiveness and device-aware choices still need measured acceptance.
SuperSplat also retains complete display textures; chunked source access does
not mean unlimited GPU residency.

## Camera, controls and automatic display

SuperSplat's [controllers][ss-controls], [camera][ss-camera],
[tween implementation][ss-tween], [settings][ss-settings] and
[editor commands][ss-editor] contain these behaviors. Our
[navigation adapter](../crates/splat/src/viewer/navigation.rs) uses shared Fly/inspect response with explicit bounds framing.

| Capability | Dimensify | Gap / distinction |
| --- | --- | --- |
| Orbit / pan / wheel zoom | Present | Left orbit, right/Shift-left pan and anchored wheel zoom through the shared controller. |
| Camera damping / smooth transitions | Partial | Shared finite orbit/pan/zoom smoothing is active; framing remains immediate. SuperSplat uses quintic focus/angle/distance transitions. |
| Fly / first-person navigation | Present | Shared Fly response, WASD/QE, speed modifiers and pose-preserving Fly/Orbit switch. |
| Adjustable fly speed / sensitivity | Missing | Our sensitivities are constants; no settings. |
| Touch orbit / pinch / pan | Missing | No touch gesture handling in our controller. SuperSplat handles one- and two-pointer gestures. |
| Trackpad / modifier conventions | Partial | Pixel/line units and signed Ctrl-orbit/Shift-pan are handled; Ctrl takes priority. Browser pinch interception and touch gestures remain absent. |
| Double-click surface focus / middle-click focus | Missing | Requires splat-aware surface picking, not just center/AABB bounds. |
| Frame whole cloud | Present | Explicit F and Frame button; loading preserves the current camera. |
| Frame selection or active tool | Missing | SuperSplat can frame selected Gaussians, a layer, or orientation points. |
| Useful initial view of an interior | Partial | Starts at origin in Fly. No saved/training-camera startup pose or automatic interior selection. |
| Outlier-resistant framing | Missing | Our bound expands all center bounds by three times the largest scale. One extreme Gaussian can dominate it. SuperSplat also uses bounds; automatic ideal interior framing is not guaranteed there either. |
| Adjustable FOV | Available | Bevy projection supports it; no example setting. SuperSplat exposes 10-120 degrees. |
| FOV Auto Dolly | Missing | SuperSplat changes distance with FOV to preserve framing. A FOV slider alone would not provide this behavior. |
| Reset view | Present | Dedicated Reset returns to origin and initial orientation and clears motion. Saved poses remain absent. |
| Axis/view cube and canonical views | Missing | SuperSplat's cube supports view alignment and orthographic views. |
| Orthographic navigation | Available | Bevy supports projection; example controller is designed around perspective and needs adaptation. |
| Near/far clipping | Partial | Our planes follow orbit distance. No scene/depth-aware clipping controls or UI. |
| Source up-axis choice | Present | All six signed axes, applied live to current and pending whole-file/streamed roots; default +Z. |
| Align by two/three surface points | Missing | SuperSplat can define vertical or a plane, align to grid, and place a local frame/pivot. |
| Saved/imported camera poses | Missing | No pose lists, thumbnails/gizmos or pose restoration in the example. |
| Camera information overlay | Missing | No camera coordinates/FOV display. |

!!! warning "Automatic display has several meanings"
    Loading successfully, choosing orientation, choosing a camera pose, framing
    useful content, adapting quality during movement and stopping idle redraws
    are separate contracts. A Frame button does not complete the other five.
    Our five-million-scene browser capture confirms rendering, but its initial
    exterior view is not an accepted interior-navigation experience.

## Scene management, editing and inspection

SuperSplat's [menu][ss-menu], [tools][ss-tools], [operations][ss-ops],
[scene list][ss-list], [data panel][ss-data] and [orientation tool][ss-orient]
show capabilities absent from the current Dimensify example.

| Capability | Dimensify | Gap / distinction |
| --- | --- | --- |
| Multiple layers, names, active layer, visibility, solo, delete | Missing | Current lifecycle owns one replacement cloud. Bevy entities alone do not provide these workflows. |
| Object position / rotation / scale fields | Available | Entity transforms exist; no editor state, fields, history or export semantics. |
| Move / rotate / scale gizmos | Missing | Also needs local/world space, snapping/pivot contracts and pointer ownership. |
| Per-Gaussian transforms | Missing | Distinct from moving the cloud root; SuperSplat uses instance/transform palettes. |
| Rectangle, brush, polygon, lasso selection | Missing | No selection model or picker in the example. |
| Sphere, sphere-brush and box selection | Missing | Requires world/screen-space masks and selection intents. |
| Eyedropper / connected flood selection | Missing | SuperSplat has both tools; no corresponding example behavior. |
| Selection add / subtract / set / intersect | Missing | Needs stable Gaussian identity and reusable selection operations. |
| Visible-depth vs through selection | Missing | SuperSplat supports both, plus centers vs footprint selection. |
| Select all / none / invert | Missing | No selection state. |
| Lock/unlock, delete selected Gaussians | Missing | SuperSplat separates flags/instances from underlying source data. |
| Duplicate / separate selected data | Missing | Needs layer creation and shared-source lifetime semantics. |
| Undo / redo | Missing | SuperSplat has operation objects and a serialized async command queue. |
| Copy/paste transforms and colors | Missing | Present in the inspected SuperSplat revision. |
| Measurement | Missing | SuperSplat has a point measurement tool and dimension labels. |
| Grid planes and bounds/dimensions overlays | Partial | Vendor can visualize bounds; no active example grid/overlay controls. |
| Histograms and property range selection | Missing | SuperSplat GPU tools inspect position, depth, RGB/HSV, opacity, scales, volume, rotation, DC and SH coefficients. |
| Selection appearance, centers, rings, outline preferences | Missing | Requires both rendering and selection state. |
| Crop button | Not a confirmed baseline | The inspected bottom toolbar comments out its dedicated crop button. Deletion/selection workflows still exist; do not count every SVG as a working tool. |

## Documents, animation, output and integration

See SuperSplat's [document model][ss-doc], [timeline][ss-timeline],
[camera poses][ss-poses], [export choices][ss-export], [serialization][ss-serialize],
[image settings][ss-image], [video settings][ss-video], [publishing][ss-publish],
[preferences][ss-prefs] and [embedding API][ss-iframe].

| Capability | Dimensify | Gap / distinction |
| --- | --- | --- |
| New/open/save/save-as editor document | Missing | SuperSplat saves resources, layer instances, view state, camera and timeline. |
| Shared resources across duplicate layers | Missing | SuperSplat avoids saving one complete baked cloud per duplicate. |
| Dirty state / unsaved navigation guard | Missing | Requires document semantics first. No evidence of automatic crash recovery should be inferred from Save. |
| Persistent preferences and per-panel reset | Missing | SuperSplat persists validated preferences, with URL overrides and scoped reset. |
| Localization | Missing | SuperSplat lists nine locales with automatic detection and live language changes. |
| Shortcut help and contextual tooltips | Partial | Ours shows a static mouse/F hint. No editor shortcut registry/help panel. |
| Compact/mobile panel layout | Missing | No responsive narrow-screen acceptance for our shared toolbar/drop overlay. |
| Camera keyframe timeline | Missing | SuperSplat has pose keys, move/copy/remove keys, frame rate, loop and spline smoothness. |
| Numbered PLY playback | Missing | A sequence workflow is separate from general 4D Gaussian support. Dormant vendor 4D types are not an active player. |
| Export standard/compressed PLY | Missing | Decoder is read-only; vendor serialization helpers are not a complete editor export path. |
| Export SOG / SPZ / SPLAT | Missing | SuperSplat exposes these, with SOG compression iterations, SPZ version and SH-band choices where applicable. |
| Bake transformed/graded/filtered data | Missing | Must preserve coordinate, quaternion and SH rotation semantics. |
| Export standalone HTML / viewer ZIP | Missing | SuperSplat can package current view and optional animation. Our WASM build is a generic viewer, not a per-scene publishing command. |
| Image capture | Missing | SuperSplat offers PNG/JPEG/WebP, custom resolution, alpha where supported and optional overlays. |
| Video capture | Missing | SuperSplat offers frame ranges, frame rate, resolution, codecs/containers with browser capability checks. Availability depends on the device/browser. |
| 360/equirectangular image and video | Missing | SuperSplat has projection and horizon options; ordinary screenshots are insufficient. |
| Hosted publishing / republishing | Missing | SuperSplat uses account/server APIs, multipart uploads and separate published-viewer settings. This is not supplied by the editor code alone. |
| Embedding API | Partial | Our example exposes no dedicated host-control API. SuperSplat's inspected iframe API is narrow: dirty query and navigation request; not a general remote editor API. |
| Realtime collaborative splat editing | Not a confirmed baseline | SuperSplat source inspected here does not establish this feature. Dimensify transport/session/hub exist separately but are not wired into the splat viewer. |
| Python-controlled/headless simulation | Separate Dimensify goal | Neither this splat example nor the inspected SuperSplat editor supplies the intended complete simulation API. Excluded legacy prototypes do not count as active support. |
| Robot/physics scene composition | Separate Dimensify goal | Reuse camera, scene and controls across examples; avoid coupling splat viewing to a simulation backend. |
| Generic mesh/point-cloud import, XR and arbitrary 4D formats | Not a confirmed baseline | Do not credit the SuperSplat editor with every PlayCanvas engine capability. Its file handler and active component configuration define its actual workflows. |

## Recommended sequence

This is a proposed order, not approval to implement every editor capability.

Before new feature work, extract applicable existing camera, packed residency
and streaming implementations into optional shared libraries. Use a separate
camera package independent of splats/physics, and extend `dimensify_splat` with
neutral source, residency and streaming modules. Keep GPU kernels/layouts in
the maintained renderer fork; consumers own asset authorization and product
policy. Reconcile renderer/helper snapshots before switching dependencies.
Preserve baseline behavior and feature isolation, then reassess the matrix.

1. **Opening and navigation:** touch/trackpad controls, surface focus, useful initial views, FOV/Auto Dolly,
   view cube and persisted camera preferences. Verify the same real interior in
   native and WASM with a preserved camera/viewport record.
2. **Large-scene residency and responsiveness:** expose the shared packed SOG/SH
   backend, budget feedback and progress in the example. Streamed chunk decode
   now has real browser worker acceptance; large captured-scene responsiveness
   and device budgets still need measurements. SuperSplat also explicitly disables splat-transform worker jobs in its
   inspected startup; do not describe it as universally nonblocking.
3. **Rendering policy:** expose applicable existing settings, establish matching
   color/blend expectations, then implement stochastic movement/Auto mode with
   clean sorted settle. Add timing and overdraw evidence before claiming speed
   or visual parity. Keep noisy fast movement an explicit tradeoff.
4. **Interoperability:** compressed PLY, SPZ/SPLAT/KSPLAT, related-file SOG and
   startup URL inputs, with format-specific fixtures and device acceptance. Prioritize
   actual user datasets rather than the longest extension list.
5. **Scene tools:** multiple cloud entities, visibility, transforms, measurements
   and orientation. Establish a document and semantic operation model before
   Gaussian selection, grading and undo/redo.
6. **Authoring/publishing:** project persistence, destructive edits, export,
   animation and capture as optional layers. Add collaboration adapters after
   stable asset identity and document operations exist.

Keep one shared Feathers UI. Reusable decoding/residency, camera, rendering and
document operations belong below it; browser/native adapters only handle their
platform boundaries. Do not put renderer dependencies into headless networking
packages. SuperSplat is a useful reference implementation, but its TypeScript
UI and PlayCanvas renderer cannot be directly installed as Bevy components.

## Evidence and remaining acceptance

Dimensify's [Feathers reproduction record](repros/splat-feathers-2026-10-23.md)
records native/WASM compilation, focused tests, headless browser file selection,
PLY/SOG rendering, failed replacement and the five-million scene. This audit
does not add a SuperSplat runtime replay, equal-quality timing comparison,
cross-browser/device matrix or native OS interaction acceptance.

[ss-files]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/file-handler.ts
[ss-loader]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/io/read/loader.ts
[ss-assets]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/asset-loader.ts
[ss-projected]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/projected-splat-renderer.ts
[ss-resource]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/editor-splat-resource.ts
[ss-scene]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/scene.ts
[ss-shader]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/shaders/projected-splat-shader.ts
[ss-controls]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/controllers.ts
[ss-camera]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/camera.ts
[ss-tween]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/tween-value.ts
[ss-settings]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/ui/settings-panel.ts
[ss-editor]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/editor.ts
[ss-menu]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/ui/menu.ts
[ss-tools]: https://github.com/playcanvas/supersplat/tree/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/tools
[ss-ops]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/edit-ops.ts
[ss-list]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/ui/splat-list.ts
[ss-data]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/ui/data-panel.ts
[ss-orient]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/tools/orient-tool.ts
[ss-doc]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/doc.ts
[ss-timeline]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/timeline.ts
[ss-poses]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/camera-poses.ts
[ss-export]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/export-options.ts
[ss-serialize]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/splat-serialize.ts
[ss-image]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/ui/image-settings-dialog.ts
[ss-video]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/ui/video-settings-dialog.ts
[ss-publish]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/publish.ts
[ss-prefs]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/preferences.ts
[ss-iframe]: https://github.com/playcanvas/supersplat/blob/ebce8e6e3f3f41f3a816f35a0f4fe27beacaae53/src/iframe-api.ts
