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
