# Visual regression implementation

Approved scope: [visual regression and OSS CI audit](visual-regression-ci-audit.md).
Work branch: `codex/visual-regression-ci`.

- [x] Activate the reviewed 21 raster/packed/turn cases and one buffer case with
  inventory and executed-count guards, serial execution and failure artifacts.
- [x] Add a shared deterministic native/WASM Gaussian appearance testbed with
  real renderer readiness and completed captures.
- [x] Add semantic region assertions and strict reviewed-baseline comparisons,
  expected/actual/diff artifacts and deliberate failure/recovery tests.
- [x] Add browser capture and explicit remote physical-device capture options.
- [x] Wire Linux correctness, browser rendering, scheduled Metal/DX12 and optional
  trusted Pixel Eagle reporting. Keep default headless graphs isolated.
- [ ] Review available baseline captures, validate local and hosted execution,
  document remaining provider/device provisioning, and update the checklist.

Implementation decisions:

- Use one 256x256 offscreen testbed for native and WASM, including browser GPU
  readback. This avoids browser UI/canvas composition differences in references.
- Keep semantic assertions runnable without a service account. Golden comparison
  is explicit and requires a reviewed matching baseline; no silent auto-approval.
- Keep external `SOG_REPLAY_DIR` reproductions in a separately provisioned lane.
- Pixel Eagle reporting is optional and separate from the mandatory local verdict.
  Physical device/account availability must be reported accurately.

Validation evidence:

- Local M5 Max/Metal: 22/22 existing GPU cases; five native appearance cases
  repeated identically from clean `a5f4dcb`. Reviewed native reference registered.
- Chromium WebGPU/Metal: five appearance cases repeated identically from clean
  `f47e7b6`, with an explicit hardware/driver profile. Reviewed browser reference
  registered; strict golden comparison passes.
- Deliberately substituting the reverse-overlap image for the forward view fails
  the semantic color/order assertion; restoring the real capture passes all
  semantic and golden checks.
- Script contracts: 44 Python tests and 72 Node tests pass. Native/WASM visual
  Clippy, five Rust helper tests, workflow lint/security checks, formatting and
  standalone/headless boundary checks pass. Hosted platform references remain
  pending the initial workflow captures.
- Pixel Eagle and physical-mobile workflows are implemented. Live provider
  verification requires project credentials/reference IDs and actual device
  access; neither is available in this repository's settings.
