# Shared camera core

`dimensify_camera` provides reusable camera values and input response on native
and WASM. It requires no window, renderer, robot parser or physics backend.
It does not install a camera writer: the consuming application chooses its
active controller, applies collision constraints and writes the final transform.

| API | Responsibility |
| --- | --- |
| `CameraPose`, `OrbitState` | Eye/orientation/lens values and semantic orbit view requests |
| `TargetPose`, `PoseTransition` | Controller target and direct eye/rotation easing for mode/focus handoff |
| `smooth_look` | Short-arc yaw and bounded pitch with a smoothed input tail |
| `FlyInput`, `fly_velocity` | Normalized keyboard momentum and separate pointer/navigation displacement |
| `WalkMotion`, `WalkBody`, `WalkInput` | Fixed-step walking, jump/suspension response, queued impulses and display interpolation |
| `WalkEnvironment`, `WalkSupport` | Consumer-supplied terrain/collision queries and delayed attachment |
| `InspectController` (`inspect` feature) | Anchored orbit/pan/zoom, finite smoothing, cancellation and semantic view requests |
| `InspectEnvironment` | Consumer-supplied surface picks and eye constraints |
| `FovConvention`, bounds/fit helpers | Authored lens conversion and viewport-aware framing |

Camera values use right-handed Bevy Y-up coordinates, radians and metres.
Canonical scene assets remain Z-up; convert at the ingestion boundary once.
Motion functions consume finite semantic input and nonnegative frame time in
seconds. Device polling, UI capture, pointer lock and coordinate conversion stay
outside the motion functions.

## Input and pose ownership

Collect device input and resolve UI/gesture ownership before calling a controller.
Pass look deltas in radians to `smooth_look`. Keep its target between frames so
zero-input frames drain the look tail; clear the target on cancellation or mode
reseed. Fly and Walk can share this look response without sharing a physics backend.

`fly_velocity` updates consumer-owned local keyboard momentum and returns a world
velocity. Keyboard direction is normalized and speed-scaled. Pointer and automatic
navigation displacements are metres for the current frame, independent of keyboard
speed and acceleration. Planar Fly levels keyboard movement while pointer pan
still follows the displayed view. A zero-duration frame applies no displacement.
Clear keyboard momentum on input suppression or mode reseed.

Apply collision constraints to the proposed movement before producing `TargetPose`.
Nonzero keyboard momentum still requires another frame when collision holds the
eye stationary; the shared cutoff drains that demand to zero. Collision geometry
and application scene lookup remain in the consumer.

Call `PoseTransition::start` with the visible pose when changing modes or accepting
a semantic view request. `advance` blends eye and orientation directly, avoiding an
arc toward a distant orbit pivot during first-person handoff. At completion it uses
the current controller target directly. Setting `timer` to one cancels the blend.
Only the consumer's final writer applies the returned pose to its camera transform.

## Walk environment adapter

`WalkMotion::advance` integrates camera locomotion at 60 Hz, interpolates the
display eye and returns whether horizontal motion or queued input needs another
frame. It consumes at most ten fixed steps per update and discards excess time.
Also request redraw when the resulting pose changes; the returned demand covers
queued work and horizontal momentum that a stationary collision-constrained eye
can hide.
Look smoothing uses the same capped interval. Keyboard motion is speed-scaled;
pointer/navigation displacement and jump edges remain queued across display
frames without a fixed step. Vertical pointer pan does not trigger a jump.

Supply a `WalkEnvironment` to test clearance, query terrain height and resolve a
fixed-step eye movement. The adapter owns capsule dimensions, spatial queries and
contact projection. It returns the resolved eye, vertical velocity and ground
contact; horizontal momentum remains in the shared controller even against a wall.
This is camera locomotion, not a robot/world physics runtime.

`WalkSupport::Held` retains the entry level until the environment reports a clear
walker. On scene collision replacement, reset support to a held level at the
current eye. On modal input capture, discard `WalkMotion` and queued look/input,
but retain vertical body state and terrain support. Reseeding from the body
preserves ascent, avoiding suspension engagement halfway through a jump.
The consumer retains its mode state, frame scheduling and sole pose writer.

## Inspect environment adapter

Enable the optional `inspect` feature to use `InspectController`. It uses the
vendored `bevy_editor_cam` motion API without installing that crate's plugins.
The feature adds camera/window metadata and redraw messages, but no renderer,
window runner, robot parser or physics engine.

Collect owned `InspectInput` with stable gesture identities and logical-pixel
deltas. A new identity rebases the anchor without moving the eye. Use
`InspectFrame` for modal suppression, selected-object pivot, collision policy,
FOV zoom and elapsed time. Supply `InspectContext` with the live pose, target,
orbit state, transition and a scratch projection borrowed from an ECS resource.
Keep the live projection separate from scratch: manipulation may change its
orthographic scale. The headless example shows the complete wiring.

`InspectEnvironment` supplies pointer surface hits and constrained eye movement;
use `()` for free motion and focus-plane anchoring. Geometry, clipping and scene
lookup stay in the consumer. The shared controller freezes a world anchor,
corrects it after constrained movement, updates the target and publishes orbit
state. Semantic view requests are consumed once before input suppression.
Released gestures drain a finite smoothing window without starting a fling.

Call `cancel` on mode exit. Request another frame while `advance` returns true,
and also for changed poses, lens values or an active `PoseTransition`.
The controller never writes a render transform. Do not run the vendor's stock
camera plugin alongside the consumer's final writer.

`InspectDrag` latches pan/orbit and gesture identity from resolved button
ownership. `pointer_drag_owned` retains release-frame movement and prevents a
held UI-owned press from acquiring the camera later. `inspect_wheel_input`
combines logical-pixel pan/orbit and wheel-line zoom. Normalize platform wheel
signs before calling it; Ctrl selects orbit before Shift selects pan. Keep modal,
viewport and browser ownership decisions in the consuming adapter.

The file splat viewer now uses these mappings and the shared inspect controller,
retaining its one pose writer and immediate bounds framing. It updates
continuously, so it also drains finite smoothing between device events.

## Headless examples and tests

From the Dimensify workspace:

```sh
cargo run --locked -p dimensify_camera --example frame_bounds
cargo run --locked -p dimensify_camera --example fly_camera
cargo run --locked -p dimensify_camera --example walk_camera
cargo run --locked -p dimensify_camera --features inspect --example inspect_camera
cargo test --locked -p dimensify_camera --features inspect
```

The Fly example drives semantic look/movement and drains to idle without creating
a window. The Walk example adds an analytic floor and a jump at 120 Hz display
rate without a physics engine. The inspect example computes orbit/pan/zoom
targets while leaving the render transform untouched. These are headless examples.
Touch/gamepad adapters, interactive Walk and shared navigation presentation
remain pending; applications provide their own scene geometry adapters.
