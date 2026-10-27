# Shared viewer foundation acceptance: 2026-10-23

## Candidate and scope

Base revision: `eeb8cac6`, with uncommitted renderer consolidation, shared camera
pose/view/lens/bounds and interpolation packages, shared streamed-manifest/LOD
policy, consumer forwarding and build-graph changes. The browser WASM example
was rebuilt and wasm-bindgen output regenerated after the last source changes.
The local preview serves that generated `splat-web/pkg` artifact on port 8087.

Renderer source/codec/sorter behavior is preserved. The file viewer now uses
shared camera values and fit math, retaining its 20 percent framing margin.
This slice does not implement the full shared navigation controller, packed
residency lifecycle, streamed file opening or quality panels.

## Reproduction and inspected evidence

Use the fixture and browser procedure from
[the Feathers reproduction](splat-feathers-2026-10-23.md): Chromium 153.0.8010.12,
macOS WebGPU/Metal, 1000 by 760 viewport, default camera, Z-up source,
normalized Gaussian profile and eight-Gaussian PLY/SOG fixtures. No visible
windows were opened. This is functional acceptance, not a performance capture.

The headless harness passed canvas Open File/system chooser, PLY loading, SOG
drop, Frame and invalid replacement. The host has no duplicate HTML controls.
Evidence is local under `/tmp/dimensify-foundation-2026-10-23/`. The inspected
`sog.png` preserves the baseline framing and colored Gaussian fixture.
`invalid.png` shows the error while retaining that fixture. There were no
reported browser console errors.

## Supporting verification and limits

The application adapter's camera, fit, cursor-presence and LOD suites passed
575 selected tests. Before extraction, seven camera/fit and eleven LOD policy
tests passed against the original implementation; the moved shared tests pass.
The shared camera/interpolation/LOD core suites pass 26 tests. The rendered
splat suites pass 28 tests, with the optional captured-scene decoder test ignored.
Strict Clippy passes for all shared package targets with the viewer feature.

Isolated export resolves all local dependencies inside the exported tree. Its
hub, camera core and policy-only splat graphs exclude rendering and physics.
The headless bounds-fit example provides a neutral camera consumer. Moon graph,
affected-plan, formatting, browser scheduling and worker-packaging checks cover
the moved source paths and package aliases. Native application checking and
application WebGPU/WebGL Clippy pass with existing warnings. Both worker
packages resolve their `wasm-release` profile and compile for WASM.

The full captured five-million scene was not replayed in this slice. Native OS
file interaction, shipped application browser scene parity, robot navigation,
other devices/browsers and release performance remain unverified. Application
backend compilation and ECS tests do not establish that runtime scene parity.

## Follow-up motion extraction

Base revision: `9fa4087`, with uncommitted camera motion/transition extraction,
consumer forwarding, semantic Fly example, tests and documentation. This moves
the existing direct cubic pose transition, short-arc look smoothing and Fly
keyboard/pointer velocity response into `dimensify_camera`. Input ownership,
collision, terrain attachment, motion suppression and the final transform writer
remain with their existing consumer. No new camera plugin is installed.

The baseline passed 42 first-person and fixed-look handoff tests before consumer
changes. The candidate passed 396 selected camera/framing/cursor tests. Shared
core/helper suites passed 38 tests, including six new motion/transition cases.
Strict camera Clippy passed for all targets, formatting passed, and the headless
`fly_camera` example ran to zero keyboard momentum. Application WebGPU and WebGL
Clippy passed with existing warnings. Isolated source export and headless camera,
splat-policy and host dependency boundaries passed. Read-only source review found
no behavior-preservation issue. Evidence logs are local under
`/tmp/dimensify-motion-*.log`.

The new tests cover direct eye interpolation, interruption/retargeting, shortest
yaw smoothing, pitch bounds, pointer frame-rate/speed independence, keyboard
normalization, finite momentum drainage and planar/view basis separation.
These checks do not add interactive Fly controls to the file viewer. No new
browser/native visual or performance capture was performed for this follow-up;
the earlier file-viewer captures describe the preceding candidate. Full inspect,
Walk terrain and device-input extraction remain pending.

## Follow-up Walk extraction

Base revision: `5d15a03`, with uncommitted shared Walk state/integration, a neutral
terrain/collision interface, consumer adapter forwarding, an analytic-floor
example and tests. The shared core retains the existing 60 Hz step, ten-step
catch-up bound, jump/suspension response, queued pointer/navigation displacement,
display interpolation and horizontal idle-demand cutoff. Geometry queries,
capsule dimensions, modal capture, scene changes and the final pose writer stay
in the consumer. This is camera locomotion, not a simulation runtime.

The unchanged twenty application Walk fixtures pass before and after extraction,
including real geometry, modal suppression, collision replacement and navigation.
All 43 shared core/helper tests pass, including five new Walk integration cases.
Strict camera Clippy passes for all targets, formatting passes, and application
WebGPU/WebGL Clippy passes with existing warnings. Isolated export resolves local
dependencies and retains the headless camera/splat-policy/host boundaries.
Read-only source review found no behavior-preservation issue.

The headless `walk_camera` example runs at 120 Hz display rate against an analytic
floor, reaches a jump peak of 2.283 m and settles at approximately 1.5 m eye height
with zero horizontal momentum. No window or physics engine is initialized.
Logs are local under `/tmp/dimensify-walk-*.log`.

No new native/browser visual or performance capture was made. Inspect gestures,
device input, interactive example composition, packed residency/cache and shared
panels remain open. Earlier captures apply to their recorded candidates.

## Follow-up inspect extraction

Base revision: `5d15a03`, with the uncommitted Walk slice above plus shared inspect
motion, semantic input and a neutral scene-query interface. The existing
editor-cam fork is moved into `vendor/`, preserving its 32 non-manifest files
byte-for-byte, licenses, examples, tests and provenance. Renderer-only extension
dependencies become optional; the inspect consumer uses only the motion API.
No stock plugin or additional camera transform writer is installed.

The original fourteen application inspect tests pass before and after extraction.
The candidate also passes 362 camera/framing integration tests, including input,
view cube, mode handoff and Walk. All 46 shared core/helper tests pass with inspect
enabled, including three new cases for frozen orbit anchors/single-writer output,
combined pan/pinch cancellation and one-shot view requests under suppression.
Strict camera Clippy passes for all targets with inspect enabled. Formatting,
application WebGPU/WebGL Clippy, isolated export and Moon graph/affected selection
checks pass. Application backend checks retain existing warnings. The export
checks both default and inspect camera graphs for renderer, window-runner, robot
and physics dependencies; neither graph includes them.

The headless `inspect_camera` example computes orbit, pan and zoom targets while
asserting that the rendering transform remains unchanged. Its empty environment
uses focus-plane anchors and unconstrained motion. Read-only source review found
no behavior-preservation issue. Maintained source, including hidden vendor files,
passes the neutral naming scan, and changed documentation links resolve.
Evidence logs are local under `/tmp/dimensify-inspect-*.log`.

No new visual, browser runtime or performance capture was performed. The app's
WebGPU/WebGL checks establish compilation, not device or scene parity. Default
vendor extensions were preserved but not independently run. Device capture,
interactive splat/robot composition, packed residency/cache and shared panels
remain open. Earlier visual evidence applies only to its recorded candidates.

## Mouse inspect integration: 2026-10-25

Base revision: `685580a2`, with uncommitted shared mouse ownership/latching and
wheel mapping, consumer forwarding and standalone navigation integration. The
file viewer uses the shared inspect controller with its sole final pose writer.
Scene picking remains optional; this viewer uses focus-plane anchors and free
motion. Framing stays immediate with its existing margin. Touch/gamepad capture,
interactive Fly/Walk and robot camera integration are outside this slice.

The preserved browser package from the initial foundation acceptance was replayed
before replacement. The candidate native and WASM examples were rebuilt after
correcting a reviewed modified-wheel direction/priority mismatch, and browser
bindings regenerated. The final source/artifact SHA-256 manifest is local at
`/tmp/dimensify-controls-final-2026-10-25/artifact.json`. It identifies the tested
dirty Rust/manifest/script inputs and served WASM/JS package. The temporary
preview served that package on localhost port 8091. No visible windows opened.

Reproduction uses Chromium 153.0.8010.12, macOS WebGPU/Metal, 1000 by 760 viewport,
the default camera and normalized Gaussian profile, Z-up source and the authored
eight-Gaussian PLY/SOG fixtures. Baseline automatic SOG framing and final candidate
framing have identical pixels. Inspected captures show the colored fixture and
changed orbit/pan/zoom views; Frame recentres it while retaining the manipulated
orientation. The headless harness exercises real canvas input and verifies Open
File/system chooser, PLY selection, SOG drop, a toolbar-owned drag that leaves the
viewport unchanged, orbit/pan/zoom viewport changes, Frame and invalid replacement.
No browser console errors were reported. Final captures and logs are local under
`/tmp/dimensify-controls-final-2026-10-25/`; the preserved baseline is under
`/tmp/dimensify-controls-baseline-2026-10-25/`.

All fifty shared core/helper tests pass, including four mouse mapping cases. All
34 rendered splat/loader tests pass, with one optional local large-input test
ignored. Six navigation fixtures cover UI-start ownership, window-focus loss,
smoothing-tail delivery, framing margin and signed modified-wheel behavior.
The old viewer fails the first three reported behavior cases; the pre-correction
adapter fails both signed modifier regressions. All 52 unchanged application
input tests pass against the shared mapping. Strict shared camera/splat Clippy,
formatting, native/WASM builds, application WebGPU/WebGL Clippy, isolated export
and focused OpenSpec validation pass. Read-only review's wheel-direction finding
was fixed and re-reviewed. Repository-wide spec validation retains the unrelated
`auth-demo-resilience` failure. Logs are local under `/tmp/dimensify-controls-*`.

The browser replay covers bare-wheel zoom and mouse drags; signed modifier cases
are verified in native ECS fixtures. Native OS file-picker/gesture use, browser
pinch interception, touch/gamepad behavior, large-scene replay, other devices,
shipped application scene parity and release performance remain unverified for
this slice. Compilation does not establish those runtime properties.

## Origin, Fly and live file basis follow-up - 2026-10-26

Base revision: `685580a29254fb7b738cf8e2c18520450e8d8485`, with uncommitted
shared input extraction plus scene/viewer changes. The exact dirty paths and
source/generated WASM/JS SHA-256 values are recorded in
`/tmp/dimensify-fly-origin-2026-10-26/artifact.json`. This follow-up deliberately
replaces automatic loading-time framing with Fly at the origin and explicit Frame.

Reproduction: Chromium 153.0.8010.12 headless, macOS Metal/WebGPU, 1000 x 760,
eight-Gaussian PLY/SOG fixtures, normalized Gaussian raster. The retained toolbar
was operated through real canvas clicks. File up axis Z/Y/Z changed and restored
the visible cloud without another load. The inspected axis captures show the
colored fixtures rotated; restored viewport pixels match. Fly W from explicit
Frame changes the viewport; Reset returns to the same origin view and remains
there after momentum would otherwise drain. The origin sees background in this
fixture, so movement acceptance uses the framed view; CPU tests separately assert
exact reset translation and orientation. Mode switch, UI-owned drag, Orbit/pan/
zoom, picker/drop and corrupt-replacement preservation also pass.

Evidence: `/tmp/dimensify-fly-origin-2026-10-26/`, including `empty.png`,
`axis-z.png`, `axis-y.png`, `axis-z-restored.png`, `camera-fly-start.png`,
`camera-fly.png`, `camera-reset.png`, `invalid.png` and `browser.log`. Captures were
inspected. Focused splat tests pass (29 library, 11 integration, one optional
local-file case ignored), strict camera/splat Clippy passes, and both native and
WASM viewer builds pass. The selected OpenSpec change validates. Native OS input,
large captured scenes, other browser/device combinations and performance remain
unverified for this follow-up. Interactive Walk, touch/gamepad and robot camera
composition remain pending.

Signed-axis follow-up: the same headless reproduction now cycles +Z, +Y, +X,
-X, -Y, -Z, +Z. It passes with evidence in
`/tmp/dimensify-six-axis-2026-10-26/`; all six source directions map to renderer
+Y with determinant-one rotations in CPU tests. Focused tests now total 41
passing, with one optional local-file case ignored. Native/WASM builds and
strict linting pass. This extends the file basis selector only.

## Direct remote resources follow-up - 2026-10-26

Base revision `60844365` plus uncommitted viewer HTTP/input changes. Exact source,
manifest, lockfile and generated WASM/JS hashes and dirty paths are recorded in
`/tmp/dimensify-url-final-2026-10-26/artifact.json`.

Reproduction: Chromium 153.0.8010.12 headless, macOS Metal/WebGPU, 1000 x 760,
eight-Gaussian PLY/SOG fixtures, normalized Gaussian raster. A separate local HTTP
origin serves the fixtures with CORS enabled, including a 302 PLY redirect and
signed query strings. Real canvas controls open the retained URL field; typing,
Command+A replacement, Command+V external clipboard paste, Enter load and Escape
cancel pass. Cancel leaves viewport pixels unchanged. Both downloaded fixtures
render with the current camera. Inspected `url-panel.png`, `url-ply.png` and
`url-sog.png` show the field and the loaded clouds. Evidence and console logs are
under `/tmp/dimensify-url-final-2026-10-26/`. Existing axis, Fly/Reset, Orbit,
local picker/drop and invalid replacement checks also pass in the same replay.

Regression tests first reproduced and then verified fixes for activation Enter
resubmission and signed-query Referer forwarding across native redirects. Other
focused tests cover HTTP errors, input bounds, latest-result rejection and axis
selection at completion. The isolated export check verifies that policy-only
splat graphs exclude renderer, HTTP client and clipboard dependencies. Native OS
clipboard interaction, remote production HTTPS hosts, large assets and other
browser/device combinations remain unverified; no performance claim is made.

Final candidate verification: 35 library and 11 integration tests pass (one optional large-file case ignored), strict native/WASM Clippy passes, native/WASM viewer builds pass, and the isolated export check passes.


## Robot inspect integration (2026-10-26)

Base `0827da0b`, with changes to the robot viewer, its camera tests and the
optional `robot` manifest/lock entry. The native URDF demo now uses the shared
inspect controller, owned drag mapping and bounds union. Only the local adapter
writes its camera transform, after geometry propagation; it publishes the
matching world transform before camera/frustum extraction. Startup and `F`
retain the established diagonal * 1.8 framing margin. A frame request interrupts
owned input; focus loss cancels motion without reacquiring a held button.

The two initial behavior tests first failed against the old camera (unfocused
movement and missing right-drag pan). A subsequent held-drag/frame test also
failed before the input-suppression correction. All 17 robot/viewer library tests
now pass, including the six camera schedule/input tests. Native Clippy passes
with two existing `bail!` macro warnings outside the changed viewer. The
standalone WebGPU example passes its WASM check with the existing codec atomic
API deprecation warning. Independent export/headless graph checks pass; the
consumer workspace metadata loads with its unchanged lockfile and `--locked`.
The selected OpenSpec change validates; global spec validation still reports
the pre-existing `auth-demo-resilience` requirement-keyword errors.

Offscreen baseline and candidate captures use the embedded demo, default joint
positions, initial auto-frame, 1100 by 800 target, development profile and native
Metal backend. Both images were inspected: the arm, camera framing and materials
remain visually consistent; the help text now describes pan. Images are
`/tmp/dimensify-robot-camera-baseline.png` and
`/tmp/dimensify-robot-camera-shared.png`. The viewport below the help text is not
pixel-identical (maximum channel difference 74); this is a bounded
visual comparison, not an exact-pixel or performance claim. The final native
binary SHA-256 is `912fda09032184e5d9dd120075ce8a58e36bd9bab65896c6c8614d4866c30fcc`; artifact metadata is
`/tmp/dimensify-robot-camera-artifact.json`. Logs use the
`/tmp/dimensify-robot-camera-*.log` prefix (the held-frame red test has its own
`/tmp/dimensify-robot-frame-red.log`). No visible window was opened for captures.

This verifies the native robot adapter, not browser robot support, interactive
Walk, touch/gamepad, shared settings panels, release performance or a new device
matrix. No new browser runtime replay was needed for this native-only controller
change; its optional dependency remains disabled in the standalone browser path.

## Touch controls and example gallery (2026-10-26)

Base `d75f9bba` with shared touch, standalone viewer and gallery changes.
The camera package now owns stable touch identity and viewport admission; hosts
retain device polling, taps and navigation policy. The standalone viewer supports
one-finger Orbit/Fly look and two-finger pan/pinch with 44-pixel toolbar targets.
Per-pointer UI hits prevent a stationary mouse over the toolbar from blocking a
finger in the scene. Both missing touch movement and stale mouse-hover suppression
were reproduced by failing tests before correction.

Verification: 27 shared camera tests, 15 standalone navigation tests and the
consumer's 50 input tests pass (two neutral tests moved into shared coverage).
The new cursor playback example runs and verifies midpoint/end values. All 13
gallery commands resolve to actual Cargo targets and source paths. Shared camera
strict Clippy, standalone WASM build, consumer WebGPU/WebGL checks, isolated export
and selected OpenSpec validation pass. Viewer Clippy completes with 16 warnings
in unchanged streaming, asset and renderer modules; its strict run remains blocked
by those warnings. The WASM build retains the codec atomic API deprecation warning.

Chromium 151.0.7922.34, headless macOS Metal/WebGPU, 1000 by 760, eight-Gaussian
PLY/SOG fixtures and the hosted hierarchy fixture pass all 19 browser checks.
Real dispatched touch events change the rendered orbit and pan/pinch views;
toolbar-started contact leaves the scene unchanged. Existing local/URL/streaming
checks pass in the same replay. Gallery filtering and a 390 by 844 layout pass
without page-level horizontal overflow. The scene comparison crop starts below
the wrapped toolbar, so button hover pixels cannot masquerade as camera motion.

Inspected gallery and touch captures, console logs and source/artifact hashes are
under `/tmp/dimensify-touch-browser-final-2026-10-26/`; `artifact.json` records the
base revision and dirty inputs. The built WASM precedes only a lint allowance in
the final navigation source, with unchanged runtime behavior. Logs use the
`/tmp/dimensify-touch-` prefix. No visible capture window was opened.

The gallery has one live browser viewer plus labelled native/headless examples.
It does not provide browser robot dynamics or interactive versions of every
console example. Physical phones/tablets, gesture tuning on those devices and
consumer runtime parity on real devices remain unverified. Gamepad expansion is
deferred. No performance, deployment or hosted CI acceptance is claimed.


## Live browser showcases, 2026-10-26

Candidate: base `f763f8f` plus the camera navigation extraction and browser
example changes recorded in the local artifact manifest. Cargo discovery now
finds 18 examples, including three live browser targets. One `just showcase-build`
builds and packages the splat viewer, built-in camera playground and shared-cube
collaboration example. The camera's optional viewer adapter owns the only final
pose write; source/modal policy and presenter samples enter through configuration.

Native verification passes 21 camera viewer tests and two collaboration example
tests. Strict Clippy passes for the camera viewer and both new example targets.
The isolated export passes its focused transport/session/hub suites and checks
that default headless graphs exclude graphical/robot/physics dependencies. All
three WASM targets and the SOG worker build. The existing splat browser replay
passes 19 checks after the adapter move.

Headless Chromium 151.0.7922.34 on macOS WebGPU replays the built-in cube/floor
scene at 1000x760 and captures collaboration layout at 390x844. The camera replay
checks Orbit, Reset, Fly and CDP touch. A local ephemeral room authority accepts
real invite-code tabs; acknowledged color and position edits render, participant
counts reach three, viewer edits are rejected, and Explore/Follow restores the
presenter view. Render readiness is checked separately from network revisions.
Inspected desktop/mobile captures show flow-layout controls and the rendered cube.

Evidence is local under `/tmp/dimensify-showcase-browser-2026-10-26/`, including
`artifact.json`, participant logs and screenshots. Splat regression evidence is
under `/tmp/dimensify-showcase-splat-2026-10-26/`. Physical phone/tablet gestures,
other browsers, native interactive windows, hosted CI and release performance
remain unverified. The gallery is not deployed; collaboration requires a reachable
authority and HTTPS pages require a secure WebSocket endpoint.


## Collaboration connection form, 2026-10-26

Candidate: base `f5e01450` plus the example connection form and ordered browser
shortcut adapter. The native and WASM example use one retained Feathers form
for host URL, display name, Create/Join and invite code. Validation precedes
connection replacement. Retry repeats the last validated attempt, preserving
the session API's endpoint-bound private resume credential instead of creating
a second participant. The shared session/transport implementations and default
headless dependency graph are unchanged.

Six native example tests pass, including endpoint/name/code validation,
mixed-content rejection, invalid-input session preservation, focus release,
ordered Command chords and the canonical coordinate round trip. Three initial
form tests failed before implementation. The browser replay exposed a shortcut
ordering failure: polling frame-final modifiers missed a rapid Command+A. The
final adapter tracks ordered focused-key events, clears on focus loss, and
keeps later ordinary text outside the chord. Strict native example Clippy and
the WASM showcase build pass. An isolated export validates catalog freshness
and the default headless dependency boundary; headless core suites were not
rerun for this example-only slice.

Headless Chromium 151.0.7922.34 verifies Create from invalid startup settings,
Join through edited fields, acknowledged rendered edits, viewer rejection,
Explore/Follow, invalid-host preservation of room and camera, and Retry restoring
the same presenter and revision. A fourth tab joins through form-entered endpoint
and code. Desktop 1000x760 and mobile 390x844 captures show the retained fields
and wrapping buttons within the viewport.

Evidence is local under `/tmp/dimensify-connection-browser-2026-10-26/`;
`artifact.json` records source/artifact hashes. Physical mobile keyboards and
gestures, other browsers, native interactive windows, hosted CI and release
performance remain unverified. This does not complete the broader camera/control
migration. Repository-wide OpenSpec validation still fails the unchanged
`auth-demo-resilience` spec; the selected change validates strictly.
