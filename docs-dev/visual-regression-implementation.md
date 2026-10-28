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
- Script contracts: 44 Python tests and 75 Node tests pass. Native/WASM visual
  Clippy, nine Rust helper tests, workflow lint/security checks, formatting and
  standalone/headless boundary checks pass.
- Hosted run `40707382978`: Linux software Vulkan passed all 22 GPU cases, strict
  Clippy/helper tests and five repeated appearance captures. macOS Metal also
  passed both five-image captures. Their initial gates failed explicitly for
  missing references; both reviewed references are now registered and pass
  strict local re-comparison. Windows WARP hit the 30-second fence timeout;
  bounded diagnostics and a phase-deadline wait are being verified. Browser
  software WebGPU is still under hosted verification.
- Pixel Eagle and physical-mobile workflows are implemented. Live provider
  verification requires project credentials/reference IDs and actual device
  access. Project UUID and token are now configured in repository settings;
  the reporting workflow must reach the default branch before service bootstrap.
  Physical hardware is unnecessary for headless correctness CI, and remains
  necessary only for representative mobile performance investigation.

Hosted software-driver follow-up:

- The first Windows WARP capture timed out on a 30-second fence. The testbed now
  retries only timeout results within its existing 90-second readiness phase,
  logs actual frame/acknowledgement progress and gives screenshot completion
  its own bounded phase. Four helper tests and strict native/WASM Clippy pass.
- Chromium SwiftShader lost the device before producing a PNG. A Linux-only
  `--disable-gpu-watchdog` configuration is being verified; the same workaround
  appears in [Three.js E2E tests](https://github.com/mrdoob/three.js/pull/33650).
  This is a hypothesis until a hosted capture passes. Explicit device errors
  must fail immediately; per-case timeouts remain bounded.

Separate pre-existing performance bootstrap remains open: pipeline run
`40707382867` measured successfully but its Bencher gate has no
`pipeline-cache-admission-v1/1024` baseline for the runner testbed. Do not treat
this as a visual result or weaken the performance gate.
