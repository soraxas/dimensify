# LLM Agent Guidelines

## Scope

- This entire subtree is an independently usable open-source component. Never
  mention downstream proprietary products, their names, branding, repository
  paths, or integration details anywhere in its source, manifests, comments,
  documentation, examples, or agent instructions. Keep downstream-specific
  guidance outside this subtree; describe shared behavior in Dimensify terms.
- Keep Dimensify viewer-first and backend-agnostic.
- Optional simulation backend is feature-gated (`nox-backend`).
- Prefer adding new APIs through small, stable layers.
- Terminology: use **stream** for scene commands, **telemetry** for sim output, **backend/plugin** for simulation runtime.
- Transport owns connections/listeners; session owns room authority and registered
  document/presence channels; hub composes schemas and hosting. Keep domain policy
  and asset authorization in consuming applications.
- Active transport/session packages select `client` and `server` features.
  Camera sharing is opt-in through session registration. Default headless graphs
  must exclude renderer, robot and physics dependencies.
- Ordered simulation streams, replication, discovery/matchmaking and WebRTC/P2P
  remain future work. The old protocol/Python and `legacy/transport` prototypes
  are excluded from the active workspace; their flags are not current APIs.
- Docs should use MkDocs Material style (callouts like `!!! note`, `!!! warning`, Mermaid diagrams when helpful).
- This file follows the AGENTS.md open format; keep new agent guidance in this file.

## Workflow

- Maintain `docs-dev/migration-checklist.md` and update it after each step.
- Add new decisions, constraints, and discoveries to the checklist.
- Keep docs in `docs/` concise and human-readable, with good examples and explanation.

## Conventions

- Default to ASCII text.
- Prefer small, focused changes.
- Avoid breaking WASM builds unless explicitly requested.

## Validation

- For viewer changes: run native or WASM build as appropriate.
- For Python bindings: ensure module import works and API surfaces are documented.
- For collaboration changes: run the focused suites and isolated export check
  documented in [collaboration](docs/collaboration.md). Browser compilation alone
  does not verify runtime connection, synchronization or resume behavior.

## Build and CI

- mise owns tool versions and Mr Boxington activation; Just owns standalone
  commands. Use `mise exec -- just <recipe>` in noninteractive shells.
- Follow [build and hosting](docs/ci.md) for checks and the static gallery.
  Keep Rust and wasm-bindgen pins aligned with their Cargo/toolchain inputs.
- Preserve separate headless/native/WASM cache lanes and job permissions.
  Publish the tested gallery artifact; do not commit generated binaries.
