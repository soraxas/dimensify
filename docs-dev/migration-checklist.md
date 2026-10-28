# CPU performance tracking (2026-10-28)

- [x] Choose Bencher CPU tracking on hosted CI; preserve renderer/GPU timing
  instrumentation for a later GPU suite. Same-repository PRs block at 20%;
  fork PRs retain artifacts without publishing.
- [x] Add failing tests for deterministic workloads, complete report ingestion,
  missing baselines and a synthetic 25% slowdown. Confirm reporting tests pass.
- [x] Implement deterministic SH3 decode, 100k/1M sort and 1k/10k LOD workloads;
  verify input correctness and the headless dependency graph. Keep the source
  export boundary check covering the new benchmark package.
- [x] Integrate separate measurement/publication jobs and main-only three-run
  baseline bootstrap. Pin Bencher 0.6.14 through mise and isolate the cache lane.
- [x] Run all five release workloads (3s warm-up, 100 samples, >=10s measurement),
  three input-correctness tests, 18 Python/script tests, focused decoder/sort/LOD
  suites, strict benchmark Clippy, formatting, Actionlint, Zizmor and isolated
  export. The pinned Bencher CLI dry-run accepts the saved output and threshold
  options; server-side adapter parsing and live regression checks are unverified.
- [x] Independently review and fix feature-branch manual runs contaminating main
  history and failed measurement jobs skipping the required gate. Verify the
  added branch-ref test and synthetic 25% regression test fail when expected.
  Forks require the measurement check; publication remains intentionally skipped.
- [x] Commit and publish `9d32eed`; hosted Linux workload validation and all five
  measurements pass. Initial publication correctly fails without configuration.
- [x] Upload that saved hosted result anonymously to create the public Bencher
  `dimensify` project (BMF v0), configure GitHub's `BENCHER_PROJECT` variable and
  validate all five parsed metrics from the live API. Confirm the gate rejects
  the initial report because three historical samples do not yet exist.
- [ ] Claim the unclaimed project and configure `BENCHER_API_KEY`, seed hosted
  baselines, verify a live PR regression and enable branch protection. The
  project claim requires the user's account; authenticated automatic reporting
  is not active until the key is configured.

# RoboSim restoration

- [x] Reproduce the missing `robot` feature and binary after Bevy migration
  `c3dc6884`; inspect the prior implementation.
- [x] Scope: optional native URDF kinematic viewer, bundled demo, joint and
  camera controls. Retired egui/editor, transport, IK, and physics integrations
  remain dormant. The historical Panda mesh assets are absent from this checkout.
- [x] Implement the feature-gated viewer and local mesh loading.
- [x] Verify loading, joint limits, hierarchy and a single Z-up conversion.
- [x] Build the native binary and check the default dependency boundary.
- [x] Document commands, supported inputs and verification limits.
- [x] Review and reproduce zero STL normals and untransformed Collada normals;
  recalculate imported normals from final geometry.
- Verification details and limits: [native reproduction](../docs/repros/robosim-2026-10-13.md).
- [x] Establish the open-source subtree boundary in `AGENTS.md` and remove
  downstream product references from maintained files.

## Build tooling

- [x] Declare Dimensify's existing workspace-inherited UI dependency in Moon
  metadata, since Moon 2.6.1 does not infer that relationship. Cargo manifests
  and feature selection remain authoritative.

## Standalone collaboration extraction

The active workspace now selects viewer markers, transport, sessions and the hub.
The 0.25 transport prototype is retained in `legacy/transport/` outside the active
graph; its Python/protocol consumers require an explicit migration before use.

Transport owns listeners and connection attempts; sessions supply the handshake
and room policy. Document/camera sharing is explicitly registered per application.
The hub composes a selected schema without owning domain data or asset access.

- [x] Preserve the existing native protocol and listener baseline.
- [x] Resolve the active workspace from an isolated source export.
- [x] Extract transport attempts/listeners without room or domain policy.
- [x] Move session authority, registration, document and presence suites.
- [x] Compose a generic host and move HTTP/TLS tests; local HTTP suites pass.
- [x] Compile the headless browser client to WASM.
- [x] Complete native/browser session acceptance and pre/post-extraction interoperability.
- [x] Run final isolated-export suites and document remaining device limits.

Validation commands and supported behavior belong in [collaboration](../docs/collaboration.md).
The export check first failed because the session package was absent; it now
resolves without parent manifests. New standalone source uses public dependencies.

The final isolated export passes the 66 generic tests and the 23-test server-only
configuration. Native and headless Chromium clients both complete invite, edit,
bidirectional presence, private identity resume and explicit leave. Clippy exits
successfully with style warnings. Browser WebTransport/TLS and full browser/device
acceptance remain outside this milestone; native tests exercise QUIC/fallback.
The HTTP test fixture waits for `/health` readiness instead of TCP acceptance.
The exported RoboSim target also passes a native compile check after narrowing
workspace defaults; no new graphical runtime capture was performed.

## Active crate examples

- [x] Add small window-free consumers for viewer camera setup, UI camera markers
  and transport endpoint validation; reuse the session check and hub executable.
- [x] Expose the two-viewer example with `collab-peer`, preserving the Dimensify
  feature in printed join commands. Document server authority and LAN usage.
- [x] Extend `just tests` to all five active workspace crates.
- [x] Move the pre-migration `custom_widgets` example to `legacy/examples/`:
  workspace testing exposed imports of the retired egui and panel registry APIs.
- [x] Verify the runnable examples, workspace tests and two-viewer acceptance.
- [x] Apply commit-hook formatting and strict workspace Clippy cleanup; rerun
  all 65 session/host tests after simplifying nested conditions.

The workspace passes 67 tests (one deployment test remains intentionally ignored).
Camera setup, UI markers and endpoint examples run successfully; invalid endpoint
input exits unsuccessfully. The headless two-client check passes invite, edit,
bidirectional presence, identity resume and leave against a separate host process.
Both graphical example tests pass, including offscreen two-client edit/follow/resume.
Inspected captures show both peers at revision 2 with the same cube and camera pose
(`/tmp/dimensify-two-peers-2026-10-15/host.png` and `viewer.png`).

The first offscreen attempt timed out before joining; a diagnostic retry passed.
The harness now waits for listener readiness on its actual bound ephemeral port,
runs scene startup before connecting, and reports session/transport errors early.
Its final run passes, but the initial timeout's cause was not conclusively identified.
Missing HUD separator glyphs were replaced with ASCII and recaptured.
Manual interaction in two OS windows, two-machine LAN use and rendered browser
behavior remain unverified; no visible windows were opened.

Commands and coverage boundaries belong in [examples and tests](../docs/examples.md).

- [x] Keep the explicit room/token viewer and add `shared_cube_invite` using short
  viewer/editor codes, sharing its scene and controls. Use standard Cargo/Just
  commands in printed instructions and the example guide.
- [x] Verify both native example builds/help output, invalid-code/argument rejection,
  shared camera tests, the network invite-code/renewal test and isolated export.
  This addition did not rerun GPU captures or browser verification.

## Mainline integration (2026-10-21)

- [x] Integrate mainline build tooling without changing the standalone boundary.
- [x] Verify all 66 transport/session/hub tests and the isolated export; Moon graph,
  planner and environment checks pass. The environment test resolves Rust selectors
  through mise rather than comparing the compiler to the literal `latest` string.
- Full rendered-viewer, browser runtime and remote deployment checks were not rerun.

## PLY and SOG splat viewer

Approved scope: an optional native/browser WebGPU viewer, independent of robot,
physics, session and hosting dependencies. A reusable `dimensify_splat` plugin
owns decoding, validated scene replacement and framing. The example owns the
Open File button, drag-and-drop zone, controls and visible loading errors.
Source coordinates default to right-handed Z-up with one root basis conversion;
Y-up files can explicitly select their source frame. Bundled SOG v2 is supported;
unbundled metadata/image directories and compressed PLY dialects are excluded.

- [x] Package generic renderer and codec dependencies inside the standalone tree;
  remove the inactive pre-migration splat adapter.
- [x] Add failing fixture tests for PLY/SOG decoding, invalid input and bounds;
  implement validated decoding and finite geometry checks.
- [x] Add scene replacement tests including failure preservation and stale results;
  implement the reusable plugin and explicit camera framing.
- [x] Add native Open File and native/browser drop loading through one request path;
  build the viewer with its empty drop zone, status and navigation controls.
- [x] Verify focused tests, native and WASM builds, isolated dependency boundaries
  and offscreen rendering; document commands and remaining runtime limits.

The 13 focused tests, strict Clippy, native/WASM builds and isolated export pass.
Headless browser acceptance and inspected captures verify file selection, PLY
rendering, SOG drop, framing and failure preservation. Native OS interaction and
large captured scenes were initially unverified. See the [reproduction record](../docs/repros/splat-viewer-2026-10-22.md).

- [x] Remove the fixed two-million-Gaussian cap after a valid five-million SOG
  was rejected. Validate declared counts against PLY payload/SOG texture capacity
  and check device storage capacity before cloud allocation. Native decoding of
  the local five-million-Gaussian input and the regression suite pass.
- [x] Verify native/WASM viewer builds and rendering with that larger input.
  Both builds and strict Clippy pass; 15 focused tests pass, plus the optional
  local five-million decode test. Headless browser acceptance loads all five
  million Gaussians without console errors; the rendered capture was inspected.

## Shared Feathers viewer UI

Use one retained Feathers toolbar, drop overlay and status view on native and
WASM. Keep file selection/read/drop adapters platform-specific. The browser
page only hosts the canvas and initializes WASM; it has no parallel viewer UI.

- [x] Verify activation tests expose the old UI's missing Feathers actions.
- [x] Share Feathers controls, source selection, file state and status across targets.
- [x] Replace HTML controls with a minimal canvas host and browser file adapter.
- [x] Verify focused tests, native/WASM builds and headless browser interaction;
  update the viewer guide and reproduction record.

All 17 focused tests, strict Clippy, native/WASM builds and isolated export pass.
Inspected headless browser captures verify the shared controls, PLY/SOG loading,
the local five-million scene and failed replacement. Native OS interaction
remains unverified. See the [Feathers reproduction record](../docs/repros/splat-feathers-2026-10-23.md).

## SuperSplat capability audit

- [x] Compare the active viewer with SuperSplat 3.6.0 source, separating exposed
  behavior, unused renderer APIs and missing capabilities. See the
  [gap audit](../docs/supersplat-gap-audit.md) for revisions, evidence and proposed
  ordering; the audit does not authorize implementing the whole editor.
- Camera damping/fly/surface focus, useful initial framing, packed residency and
  adaptive rendering are distinct gaps. Stochastic transparency needs a new
  render path; existing encoded compositing is not equivalent. Multi-LOD import
  selection is not continuous runtime streaming. Keep future tools optional and
  preserve the shared native/WASM UI and headless dependency boundaries.

## Shared viewer foundation direction

- [x] Revise the planned order to extract existing reusable implementations
  before building missing viewer features; the audit describes the standalone
  example and is not an inventory of every consumer.
- Target ownership: generic GPU kernels/layouts and compact upload primitives
  remain in the maintained renderer fork; an optional camera package owns
  navigation/pose behavior; `dimensify_splat` owns neutral source resolution,
  residency, cache/LOD/activation policy and diagnostics. Consuming applications
  retain authorization, product defaults and domain workflows. The pose/view/lens/bounds core and manifest/LOD targeting are extracted;
  full navigation and shared controls migration remain pending. Source/residency
  migration is completed in the streaming slice below.
- Reconcile renderer/codec/sorter snapshots and Bevy feature/type compatibility
  before migration. Preserve one camera writer, native/WASM Feathers controls,
  existing backend behavior and isolated headless graphs. Stochastic rendering
  and FOV Auto Dolly remain new work after extraction acceptance.

Shared-source extraction now uses one maintained renderer/codec/sorter under
`vendor/`. Original licenses, tools and fixtures are retained. `dimensify_camera`
provides pose/view/lens/bounds APIs and a headless bounds-fit example;
`dimensify_interpolation` owns the bounded playback queue. `dimensify_splat`
exposes manifest validation and LOD targeting without its `render` feature.
The core graphs remain independent of rendering and physics. Full device/navigation composition and shared quality controls remain
pending; this progress does not complete the viewer-foundation milestone.

Verification and remaining runtime limits are recorded in the
[foundation reproduction](../docs/repros/viewer-foundation-2026-10-23.md).

The [camera core](../docs/camera.md) also owns direct pose-transition easing,
smoothed first-person look and Fly velocity response. Consumer input capture,
collision/terrain adapters and the sole final camera writer retain their existing
ownership. A headless Fly example exercises semantic movement and idle drainage;
the local-file viewer now consumes this Fly response as described below.

Walk fixed-step integration, jump/suspension, queued impulses and display
interpolation are also shared. `WalkEnvironment` supplies clearance, terrain and
collision resolution without requiring a physics engine. The headless Walk
example uses an analytic floor. Consumers retain scene geometry, input capture,
mode policy and the final camera writer. Device-input migration and
interactive Walk composition in the file viewer remain pending.

Inspect anchored orbit/pan/zoom, finite smoothing, cancellation and semantic view
requests now live behind `dimensify_camera/inspect`. Its scene interface leaves
surface picking, clipping and collision geometry with consumers. The vendored
motion engine and its provenance move inside `vendor/`; renderer-only extensions
are optional so the inspect graph remains headless. A headless example computes
targets without writing a camera transform. Device capture and interactive viewer
composition remain pending; this is partial viewer-foundation progress.

Mouse inspect integration now uses shared button ownership, gesture latching and
wheel mapping in both consumers. The file viewer forwards to the shared inspect
controller with finite smoothing and keeps its sole final transform writer.
Explicit framing retains its existing margin. Touch capture is recorded below; interactive
Walk remains open; robot integration is recorded below. See the
[foundation reproduction](../docs/repros/viewer-foundation-2026-10-23.md) for
headless browser evidence and remaining runtime limits.

### Local viewer origin, Fly and live file basis

The file viewer starts in Fly at the origin, preserves its view on load, and
exposes Reset, explicit Frame and a pose-preserving Fly/Orbit switch. Fly consumes
the shared momentum/look helpers; Reset and focus/UI suppression clear motion.
File up axis now reorients the current root and recomputes conservative bounds
from original geometry, also updating pending loads without decoding again.
Browser acceptance exercises these controls through real canvas input.

File up axis accepts all six signed axes. Each maps to renderer +Y with a proper rotation, preserving handedness; repeated changes always use original source bounds.

### Direct remote splat resources

The standalone viewer has one shared Feathers Open URL panel. HTTP(S) downloads
retain signed query strings, follow redirects, bound incoming bytes, and publish
through the same latest-selection queue as local files. Axis selection is applied
at read completion. Native uses optional Reqwest/Rustls on a worker; browser uses
Fetch streams and the resource host's CORS policy. This supports direct whole-file
PLY/bundled SOG resources, not provider authentication. Hosted hierarchy streaming is added below.

The URL field supports browser Command shortcuts through a thin input adapter; native OS clipboard support is enabled only by the viewer. Enter opening the panel cannot resubmit a retained URL. Native redirects disable automatic Referer headers so signed query strings are not forwarded.

### Shared production streaming extraction

- [x] Move neutral LOD policy/settings out of presentation ownership.
- [x] Validate the extracted controller, cache/residency and renderer readiness adapters independently.
- [x] Switch both consumers to the shared pipeline and verify hosted-manifest replacement.
- Preserve requested/submitted/displayed ownership, bounded requests and existing renderer backends. Provider authorization, product presets and progress presentation stay with consumers.

- [x] Move neutral source/byte resolution, production scheduling, worker decode,
  cache/budget admission, snapshot/resident/packed adapters and diagnostics into
  `dimensify_splat`; remove migrated duplicate implementations.
- [x] Keep product-independent core graphs headless, preserve explicit WebGPU/WebGL
  feature selection, and support native streaming without selecting browser UI.
- [x] Refresh the gap audit and hosted-hierarchy instructions. Local folder import,
  new codecs, stochastic rendering, Auto Dolly and shared quality panels remain
  later slices.

The [streaming reproduction](../docs/repros/splat-streaming-2026-10-26.md)
records source/artifact identity, unit/codec checks, offscreen renderer handoffs
and real headless browser replacement/worker/axis acceptance. This does not
complete camera/device migration or claim captured-scene/performance parity.

### Shared quality observations

- [x] Move quality/memory aggregation and bounded leaf snapshots into
  `dimensify_splat::diagnostics::quality`; install the same observer for custom
  hosts and the standalone streaming viewer.
- [x] Preserve renderer-ready versus requested counts, labelled allocation versus
  estimate/reservation, bounded evidence, opt-out cleanup and unchanged-report
  change detection. Host presets and presentation remain outside this observer.
- Shared quality panels remain future composition work; a shared report does not
  claim a complete shared settings UI or new rendering modes.

### Current architecture boundary

- [x] Audit the active workspace and module graph, then update the existing
  architecture/overview to separate reusable viewer/session behavior from
  consumer policy and UI. Mark excluded protocol/Python/legacy sources as
  prototype designs rather than supported launch APIs.
- [x] Keep the headless host independent of graphics/robot/physics and document
  opt-in renderer/streaming/viewer layers. Correct the README to describe
  origin/Fly, explicit framing and hosted sources.
- This is a useful extraction boundary, not completion of the simulation
  roadmap. Shared quality panels, full camera/device composition, additional
  formats/rendering modes and Python/physics launch contracts remain open.

### Robot camera integration

- [x] Replace the native robot viewer's duplicate orbit transform math with
  shared inspect orbit/pan/zoom, drag ownership and bounds union. Keep initial
  framing and explicit `F` requests, with one final camera writer after robot
  geometry propagation.
- [x] Verify focus-loss cancellation, held-button ownership on focus restore,
  right-drag pan, zoom/orbit and same-frame framing with the real viewer schedule.
- [x] Keep `dimensify_camera/inspect` optional under `robot`; the default host and
  camera/policy graphs retain their existing dependency boundaries.
- Interactive Walk and shared quality controls remain unfinished. Touch progress
  is recorded below; gamepad expansion is deferred.
  The [foundation reproduction](../docs/repros/viewer-foundation-2026-10-23.md)
  records native offscreen and focused verification evidence.

### Touch-first examples and gallery

- [x] Share stable one/two-finger motion and viewport ownership, preserving
  identity rebasing, UI ownership and interrupted-contact behavior. Keep host
  tap/navigation/joystick policy outside the shared tracker.
- [x] Connect touch input to the standalone Orbit/Fly viewer, suppress synthetic
  mouse duplication, expose touch hints and use 44-pixel toolbar targets.
- [x] Catalog every active feature package, add a cursor playback example and
  provide a static gallery with Browser/Native/Headless filters and a live
  splat-viewer entry point. Browser robot showcases can expand later;
  console examples are labelled as console examples.
- Prioritize touch and real phone/tablet acceptance. Gamepad expansion is
  deferred; existing downstream gamepad behavior is retained during extraction.
- [ ] Replay picker, Orbit/Fly, two-finger pan/pinch, toolbar ownership and
  contact interruption on physical phones/tablets; browser-emulated touch does
  not establish device compatibility.
- [x] Discover Cargo examples automatically and move card overrides into each
  owning manifest. Opt browser targets into one WASM/worker packaging command;
  keep native/headless targets honest and check catalog freshness on export.
- [x] Add live camera and two-tab collaboration examples through the same
  discovery/build path; preserve shared controls and local host prerequisites.
- [x] Extract the tested standalone device adapter into opt-in camera viewer
  navigation. Splat loading/modal adapters only publish bounds and capture state;
  the example shell composes shared Feathers controls and a flow-layout parent.

- [x] Add the example-owned native/WASM connection form: validate host/name/code
  before replacing a connection; Retry uses the existing endpoint-bound private
  resume credential. Keep schema, authority and transport ownership unchanged.
  Verify form input/camera capture, invite-code join and retry in a real browser.

### Standalone build, CI and public gallery

- [x] Pin mise Rust/tooling and enable the local Mr Boxington wrapper. Retain
  Just as the standalone task entrypoint and separate compiler cache lanes.
- [x] Add first-party formatting, strict native/headless/WASM Clippy, focused
  tests, export boundaries and workflow linting to pull-request/main CI.
- [x] Package all three browser examples and SOG worker assets into a complete
  static gallery; reject incomplete builds and preserve project-relative URLs.
- [x] Add headless static-site acceptance, PR artifacts and Pages deployment
  of the tested artifact after all checks pass. Static hosting does not supply
  a collaboration authority; an HTTPS client needs its own secure endpoint.
- [x] Verify release WASM packaging, strict native/headless/WASM Clippy,
  focused tests and the project-subpath gallery. Replay browser camera,
  collaboration and splat/hosted-worker acceptance headlessly. Physical-device
  touch acceptance remains tracked above.
- [x] Group routine GitHub Actions updates into one weekly PR; group security
  fixes separately and avoid enabling an unrequested Cargo update stream.
- [x] Expose lint, tests, RoboSim validation, native example builds and export
  verification as named CI steps while retaining the aggregate local commands.
- [x] Separate unoptimized native verification from interactive development:
  the [first hosted run](https://github.com/soraxas/dimensify/actions/runs/40438666025)
  passed lint/tests but exhausted its hour rebuilding
  optimized Bevy dependencies for RoboSim. Keep release gallery quality intact.
- [x] Make localhost transport fixtures use blocking accepted sockets explicitly;
  inherited nonblocking mode can drop delayed headers on macOS.
- [x] Enable Pages and publish the [example gallery](https://cs.tinyiu.com/dimensify/).
  Inspect the [successful hosted checks and deployment](https://github.com/soraxas/dimensify/actions/runs/40445725236),
  verify public asset responses and WASM content types, and replay camera
  orbit/reset/fly and browser touch controls on the deployed site headlessly.
