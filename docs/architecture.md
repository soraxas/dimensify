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

## Modes

- **Viewer-only**: local/file/TCP/DB stream ingest, no simulation.
- **Sim mode**: backend publishes telemetry + scene commands into the same stream.
