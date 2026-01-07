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
- **plugins**: host backend plugin registry and optional integrations.
- **sim**: feature-gated simulation runtime.

## Protocols

Dimensify uses two tiers of scene commands:

- **WKT (well-known types)**: fixed binary layouts for hot-path primitives (lines, text, meshes, transforms).
- **Arbitrary command payloads**: opaque binary with metadata for custom or experimental features.

Binary structs use `zerocopy` to allow zero-cost serialization/deserialization.

## Collaboration (planned)

- **Canonical stream**: all authoritative edits and sim outputs are written to the stream.
- **Replication**: optional live transport for multi-user collaboration; used to send inputs/edits.
- **Viewer behavior**: viewers render by tailing the stream (live replay).

### Transport direction

- **dimensify_transport**: optional Lightyear-backed transport (default-features off).
- **dimensify_hub**: optional collaboration layer (uses transport).
- Replication events are translated into stream commands at the server.
- Viewer-side bridge applies `ViewerRequest` messages to the command log and scene state.

## Modes

- **Viewer-only**: local/file/DB stream ingest, no simulation.
- **Sim mode**: backend publishes telemetry + scene commands into the same stream.

## Viewer modes

- **2D viewer**: uses `Camera2d`; rejects 3D-only commands with clear errors.
- **3D viewer**: uses `Camera3d`; accepts 3D scene commands.

!!! note
    Current 3D command support: Mesh3d, Line3d, Transform. Text3d and Binary payloads
    are parsed but not rendered yet.
