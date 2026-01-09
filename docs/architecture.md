# Architecture

!!! note
    Dimensify is **viewer-first**. Simulation is optional and feature-gated.

## Core structure

```text
stream/      # sources: local/file/db, replay log
viewer/      # rendering, ECS, scene representation
protocol/    # command/telemetry schema types (contract)
plugins/     # optional plugin traits + registry
sim/         # behind feature: nox-backend, physics integration
```

## Responsibilities

- **stream**: ingest scene command stream from local, file, or DB sources.
- **viewer**: render and manage scene state.
- **protocol**: define schema types for commands + telemetry.
- See `docs/protocol.md` for action-based command design details.
- **plugins**: host backend plugin registry and optional integrations.
- **sim**: feature-gated simulation runtime.

## Protocols

Dimensify uses two tiers of scene commands:

- **WKT (well-known types)**: fixed binary layouts for hot-path primitives (lines, text, meshes, transforms).
- **Arbitrary command payloads**: opaque binary with metadata for custom or experimental features.

Binary structs use `zerocopy` to allow zero-cost serialization/deserialization.

Scene actions are expressed as `WorldCommand` (spawn/insert/update/remove/despawn/clear)
with attached `Component` payloads.

Telemetry is stored outside ECS (Rerun/Arrow). The viewer reads telemetry slices and renders
them without treating them as authoritative world state.

See `docs/protocol.md` for the command vs telemetry mental model.

!!! note
    Lightyear transport is sufficient for control + viewer commands. A dedicated telemetry layer

```text
(Impeller-like or Rerun) is planned for high-rate data streams.
```

## Bevy component wrappers

Dimensify keeps protocol types Bevy-free. For Bevy-side wrappers, derive `DimensifyComponent`
to map a component to a protocol `Component` variant and send it over transport.

```rust
use bevy::prelude::*;
use dimensify_component_derive::DimensifyComponent;

#[derive(Component, Clone)]
pub struct Line3dComponent {
    pub points: Vec<[f32; 3]>,
    pub color: [f32; 4],
    pub width: f32,
    pub name: Option<String>,
}

#[derive(DimensifyComponent, Clone)]
#[dimensify(command = "Line3d")]
pub struct Line3dWrapper {
    pub name: Option<String>,
    pub points: Vec<[f32; 3]>,
    pub color: [f32; 4],
    pub width: f32,
}
```

Use `#[dimensify(into)]` on fields that need `Into` conversion from Bevy types.

## Telemetry direction (planned)

- **Log-path model**: adopt Rerun-style hierarchical log paths for telemetry naming.
- **Control vs telemetry split**: Lightyear handles viewer control/commands; telemetry is a separate layer.
- **Schema discovery**: prefer self-describing payloads (Rerun-style) over a separate registry; keep a registry option for large-scale streaming.
- **History queries**: support `latest_at` (current state) and `history` (time-range) once telemetry storage exists.
- **Multi-producer/multi-consumer**: telemetry transport should support multiple writers and viewers without coupling to the viewer process.

## Collaboration (planned)

- **Canonical stream**: all authoritative edits and sim outputs are written to the stream.
- **Replication**: optional live transport for multi-user collaboration; used to send inputs/edits.
- **Viewer behavior**: viewers render by tailing the stream (live replay).

### Transport direction

- **dimensify_protocol**: optional Lightyear-backed transport (default-features off).
- **dimensify_hub**: optional collaboration layer (uses transport).
- Replication events are translated into stream commands at the server.
- Viewer-side bridge applies `ProtoRequest` messages to the command log and scene state.

## Modes

- **Viewer-only**: local/file/DB stream ingest, no simulation.
- **Sim mode**: backend publishes telemetry + scene commands into the same stream.

## Viewer modes

- **2D viewer**: uses `Camera2d`; rejects 3D-only commands with clear errors.
- **3D viewer**: uses `Camera3d`; accepts 3D scene commands.

!!! note
    Current 3D command support: Mesh3d, Line3d, Transform. Text3d and Binary payloads
    are parsed but not rendered yet.
