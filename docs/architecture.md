# Architecture

!!! note
    Dimensify is **viewer-first**. Simulation is optional and feature-gated.

## Core structure

```text
stream/      # sources: local/file/tcp/db, replay log
viewer/      # rendering, ECS, scene representation
protocol/    # command/telemetry schema types (contract)
plugins/     # optional plugin traits + registry
sim/         # behind feature: nox-backend, physics integration
```

## Responsibilities

- **stream**: ingest scene command stream from local, file, TCP, or DB sources.
- **viewer**: render and manage scene state.
- **protocol**: define schema types for commands + telemetry.
- **plugins**: host backend plugin registry and optional integrations.
- **sim**: feature-gated simulation runtime.

## Protocols

Dimensify uses two tiers of scene commands:

- **WKT (well-known types)**: fixed binary layouts for hot-path primitives (lines, text, meshes, transforms).
- **Arbitrary command payloads**: opaque binary with metadata for custom or experimental features.

Binary structs use `zerocopy` to allow zero-cost serialization/deserialization.

## Modes

- **Viewer-only**: local/file/TCP/DB stream ingest, no simulation.
- **Sim mode**: backend publishes telemetry + scene commands into the same stream.

## Viewer modes

- **2D viewer**: uses `Camera2d`; rejects 3D-only commands with clear errors.
- **3D viewer**: uses `Camera3d`; accepts 3D scene commands.

!!! note
    Current 3D command support: Mesh3d, Line3d, Transform. Text3d and Binary payloads
    are parsed but not rendered yet.
