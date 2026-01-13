# Architecture

!!! note
    Dimensify is **viewer-first**. Simulation is optional and feature-gated.

## Core structure

```text
crates/dimensify/        # viewer, ECS, stream ingestion, telemetry store
crates/protocol/         # command/telemetry schema types (contract)
crates/transport/        # Lightyear transport (feature-gated)
crates/component_derive/ # derive macro for wrapper components
crates/widgets/          # widget command stream + UI helpers
crates/dev_ui/           # dev UI overlays (optional)
crates/hub/              # collaboration hub (planned)
```

## Responsibilities

- **viewer**: render and manage scene state, apply command log entries.
- **stream**: ingest scene command stream from local, file, or DB sources.
- **protocol**: define schema types for commands + telemetry.
- See `docs/protocol.md` for action-based command design details.
- **transport**: Lightyear-backed command transport (feature-gated).
- **widgets/dev_ui**: optional UI command streams and tooling.
- **hub**: collaboration hub (planned).

## Protocols

Scene actions are expressed as `WorldCommand` (spawn/insert/update/remove/despawn/clear)
with attached `Component` payloads. Local replay uses JSONL, one command per line,
and transport uses Lightyear message serialization.

POD structs (`Vec2`/`Vec3`/`Vec4`/`Quat`) use `[f32; N]` layouts and `zerocopy` to
support fast serialization.

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

#[derive(DimensifyComponent, Clone)]
#[dimensify(command = "Transform")]
pub struct TransformWrapper {
    #[dimensify(into)]
    pub translation: Vec3,
    #[dimensify(into)]
    pub rotation: Quat,
    #[dimensify(into)]
    pub scale: Vec3,
}
```

Use `#[dimensify(into)]` on fields that need `Into` conversion from Bevy types.

## Telemetry direction (planned)

- **Current**: telemetry is JSONL file replay with a bounded in-memory store.
- **Log-path model**: adopt Rerun-style hierarchical log paths for telemetry naming.
- **Control vs telemetry split**: Lightyear handles viewer control/commands; telemetry is a separate layer.
- **Schema discovery**: prefer self-describing payloads (Rerun-style) over a separate registry; keep a registry option for large-scale streaming.
- **History queries**: `latest_at` is supported in the in-memory store; `history` (time-range) is planned.
- **Multi-producer/multi-consumer**: telemetry transport should support multiple writers and viewers without coupling to the viewer process.

## Collaboration (planned)

- **Canonical stream**: all authoritative edits and sim outputs are written to the stream.
- **Replication**: optional live transport for multi-user collaboration; used to send inputs/edits.
- **Viewer behavior**: viewers render by tailing the stream (live replay).

### Transport direction

- **dimensify_transport**: optional Lightyear-backed transport (default-features off).
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
    Current 3D command support: Mesh3d, MeshMaterial3d, Transform, Name.
    `WorldCommand::Update` and `WorldCommand::Clear` are not implemented yet.
