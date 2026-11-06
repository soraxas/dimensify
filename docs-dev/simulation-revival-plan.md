# Simulation and robotics revival plan

Revive the uncompiled prototype modules in `crates/dimensify/src` (physics,
collision, robot control, telemetry, services, UI) on top of the active
robosim viewer, one supported contract at a time. The prototype sources stay
in place as reference until each replacement merges.

## Decisions

- **Robosim is the foundation.** It loads real URDFs (fixed, revolute,
  continuous, prismatic and mimic joints; STL/OBJ/Collada; `package://`) from
  disk or HTTP(S), natively and in the browser. The prototype `robot/` loader
  duplicates it with fewer checks and a hard-coded joint position
  (`robot/sync_state.rs:136`).
- **Rapier, used directly (`rapier3d`), not `bevy_rapier`.** Multibody
  (reduced-coordinate) joints store exactly the URDF joint coordinates, so
  physics produces joint values and robosim remains the single transform writer.
  Using the engine crate avoids waiting on a Bevy plugin release for each Bevy
  upgrade (`bevy_rapier3d 0.36` pins `rapier3d =0.35.0-glamx0.2` for Bevy 0.19).
  Avian remains a reasonable choice for game-style dynamics, not robot arms.
- **Backend seam.** `AGENTS.md` requires a feature-gated, backend-agnostic
  simulation backend. A new `dimensify_sim` crate defines that contract (model
  in, joint commands in, joint state and contacts out, fixed step); Rapier is
  its first backend behind a `rapier` feature. Default and headless graphs stay
  free of physics dependencies.
- **Collision checking uses `parry3d` directly**, queried at given joint
  configurations, independent of the dynamics backend.

## Module verdicts

| Prototype module | Verdict |
| --- | --- |
| `robot/` loader, FK, `robot/ui`, `coordinate_system/`, `camera/`, `sim/`, `plugins/`, `bin/*` | Replaced by robosim and `dimensify_camera` |
| `physics/`, `robot/physics.rs` | Redesign as the Rapier backend |
| `collision_checker/` | Rewrite on parry with an allowed-collision matrix; the prototype uses a pre-0.25 Rapier API, never applies adjacency ignores and puts every link on one body |
| `robot/control` (Jacobian IK via `k`) | Port onto robosim's model with a named end-effector link |
| `robot/visual` | Port the visual/collision toggle and colouring; redesign colliding-link display |
| `telemetry/` | Port; self-contained |
| `stream/`, `services/`, `ui/`, `crates/ui` widgets | Defer until protocol/transport revival and an egui decision |
| `pointcloud/` | Rewrite or drop; depends on a third-party crate's Bevy support |

## Phases

| Phase | Work | Acceptance | Effort |
| --- | --- | --- | --- |
| W | Browser robosim: URL/`package://` loading, GitHub file links, gallery presets and URL form | Unit tests with an in-memory transport; native `--check`/`--screenshot` of real Franka URDFs; offline browser acceptance in CI | M (done in this branch) |
| R1 | Load collision geometry beside visuals | Collision counts match the URDF; visuals unchanged | S |
| R2 | `dimensify_sim` contract and Rapier multibody backend: fixed/floating base, PD joint drives, gravity, fixed step | Headless: settles without drift, drives reach targets, limits hold; WASM builds | M |
| R3 | Parry self-collision checker: adjacency plus optional SRDF disabled pairs | Adjacent links never collide; folded pose collides; dual-arm overlap detected | M |
| R4 | Visual/collision toggle and colliding-link colours | Bevy app tests | S |
| R5 | IK on robosim's model, replacing `k`/nalgebra conversions or wrapping `k` | Reachable targets within tolerance; unreachable reported; limits respected | M |
| R6 | Telemetry store, recording and playback | Pure unit tests | M |
| R7 | Stream/services/UI with protocol/transport | Separate plan | L |

R1, R2 and R3 are independent. R4 needs R1 and R3; R5 needs only robosim.
Robosim's orbit currently ignores touch; moving it onto the shared `navigate`
controller (architecture audit item 7) brings touch support to phones.
