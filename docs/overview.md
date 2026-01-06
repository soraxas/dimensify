# Overview

!!! note
    Dimensify is **viewer-first**. Simulation is optional and feature-gated.

Dimensify consumes a replayable **stream** of scene commands (and optional telemetry). By default it runs as a pure visualizer. A simulation backend can be enabled via plugins.

## Modes

- **Viewer-only**: render from local/file/TCP/DB streams.
- **Sim mode**: backend publishes telemetry + scene commands into the same stream.

## Data source configuration (native)

- `DIMENSIFY_DATA_SOURCE`: `local` | `file` | `tcp` | `db`
- `DIMENSIFY_FILE`: JSONL replay file (for `file` source)
- `DIMENSIFY_TCP_ADDR`: `IP:PORT` (for `tcp` source)
- `DIMENSIFY_DB_ADDR`: `IP:PORT` (for `db` source)

## Philosophy

- Viewer-first: keep visualization stable and backend-agnostic.
- Feature-gated compute: opt into Nox/XLA when needed.
- Schema stability: standardize stream/telemetry types early.
- Multi-target: support native + WASM with consistent APIs.
