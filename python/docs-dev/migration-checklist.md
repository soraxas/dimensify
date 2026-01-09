# Migration Checklist

## Steps

- [x] Add `compile_info` module function returning build metadata and enabled features.

## Decisions

- Use compile-time package name/version and cfg-gated feature list.

## Constraints

- Keep compile info viewer-first and backend-agnostic by only reporting transport feature flags.

## Discoveries

- `docs-dev/migration-checklist.md` did not exist in this repo and was created.
