# Hosted stream fixture

Synthetic eight-Gaussian fixture derived from the sibling `tiny.sog` archive.
Its metadata and WebP images are unchanged; `lod-meta.json` adds a one-level
hierarchy referencing the extracted chunk. Browser acceptance serves these files
from a separate HTTP origin with CORS and records every resource request.

This small fixture verifies URL resolution, worker decode, source basis and
failure-preserving replacement. Multi-level refinement, bounded admission and
renderer handoff use the preserved controller and offscreen test fixtures.
