# Experimental stochastic rendering implementation plan

**Goal:** Add an opt-in stochastic transparency setting; sorted alpha remains the default.

**Approved design:** The user approved Sorted / Stochastic rendering settings and separate visual/performance verification in chat. This implements stochastic rasterization, not MCMC training. Automatic motion switching is deferred until explicit-mode quality is measured.

**Architecture:** CloudSettings selects transparency independently of sorting precision. Stochastic fragments use deterministic per-pixel/per-splat coverage, opaque color and reverse-Z depth writes. Stochastic vertex shaders index source records directly and issue a direct draw, bypassing radix dispatch and stale indirect counts; switching back resumes fresh radix sorting. The initial mode uses a fixed sample, exposing grain honestly without promising temporal convergence. Require a consistent mode across clouds sharing a view; viewer applies it to all current and pending clouds.

**Constraints:** Native and WASM compile; no default image changes, generated references or model assets in Git. Record render mode in benchmark identity, and never publish stochastic results into sorted history. Headless correctness does not establish mobile speed.

- [x] Add failing GPU acceptance for stochastic overlap, reordered primitives, stable captures and sorted/stochastic/sorted switching.
- [x] Implement renderer setting, direct source draws, shader coverage and specialized depth/blend state. Validate native GPU cases.
- [x] Expose viewer toolbar setting, native argument and browser query; apply to newly loaded/streamed clouds. Add setting tests.
- [x] Add explicit native LOD runner mode, preserve default sorted publication, isolate experimental reports; test argument/aggregation validation.
- [x] Run focused CPU/GPU assertions, native/WASM checks and script/workflow checks. Document limitations and update migration checklist.
- [x] Commit tested implementation and integrate locally.
