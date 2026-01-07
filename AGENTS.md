# LLM Agent Guidelines

## Scope

- Keep Dimensify viewer-first and backend-agnostic.
- Optional simulation backend is feature-gated (`nox-backend`).
- Prefer adding new APIs through small, stable layers.
- Terminology: use **stream** for scene commands, **telemetry** for sim output, **backend/plugin** for simulation runtime.
- Collaboration plan: stream is canonical; replication is optional live transport (planned Lightyear).
- Feature flags: `transport`/`dimension_transport` for `dimensify_transport`, `hub` for `dimensify_hub`.
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
