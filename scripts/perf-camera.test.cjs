const { test } = require("node:test");
const assert = require("node:assert/strict");
const { referenceFromRuns, referenceHash, validateCameraReport } = require("./perf-camera.cjs");
const pose = { position: [0, 0, 3], rotation_xyzw: [0, 0, 0, 1], fov_radians: .8, near: .01, far: 100 };
function report() {
  return { metadata: { asset: { identity: "sha256:fixture" }, viewport: { width: 1000, height: 760 }, browserEnvironment: { dpr: 1, canvas: { width: 1000, height: 760 } } },
    protocol: { durationMs: 500, steps: [{ name: "moving-main", action: "turn-return" }], cameraReplay: "input-checkpoints-v1" },
    runs: ["cold", "warm"].map(mode => ({ mode, cases: [{ name: "moving-main", began: 0, ended: 1000, settings: { splat_budget: 100 }, cameraCheckpoints: ["before", "outbound-15", "outbound-30", "return-15", "return-30", "after"].map((label, i) => ({ label, observedAtMs: i * 100, camera: structuredClone(pose) })) }] })) };
}
test("camera checks catch intermediate path drift despite identical endpoints", () => {
  const baseline = report(), candidate = report();
  candidate.cameraReference = referenceFromRuns(baseline); candidate.protocol.cameraReferenceSha256 = referenceHash(candidate.cameraReference);
  candidate.runs[0].cases[0].cameraCheckpoints[2].camera.position[0] = .5;
  assert.throws(() => validateCameraReport(candidate), /Camera drift/);
});
test("camera comparisons accept quaternion sign equivalence but reject changed projection", () => {
  const candidate = report(); candidate.cameraReference = referenceFromRuns(candidate); candidate.protocol.cameraReferenceSha256 = referenceHash(candidate.cameraReference);
  candidate.runs[0].cases[0].cameraCheckpoints[1].camera.rotation_xyzw = [0, 0, 0, -1];
  validateCameraReport(candidate);
  candidate.runs[0].cases[0].cameraCheckpoints[1].camera.fov_radians += .01;
  assert.throws(() => validateCameraReport(candidate), /Camera drift/);
});
test("references require identical asset, viewport, replay and settings", () => {
  for (const damage of [r => r.metadata.asset.identity = "other", r => r.metadata.viewport.width++, r => r.metadata.browserEnvironment.dpr++, r => r.metadata.browserEnvironment.canvas.width++, r => r.protocol.durationMs++, r => r.protocol.steps[0].action = "hold", r => r.runs[0].cases[0].settings.splat_budget++]) {
    const candidate = report(); candidate.cameraReference = referenceFromRuns(candidate); candidate.protocol.cameraReferenceSha256 = referenceHash(candidate.cameraReference); damage(candidate);
    assert.throws(() => validateCameraReport(candidate), /identity|inventory|settings/);
  }
});
test("missing checkpoints, malformed poses and missing cases cannot pass", () => {
  for (const damage of [r => r.runs[0].cases[0].cameraCheckpoints.splice(2, 1), r => r.runs[0].cases[0].cameraCheckpoints[1].camera.position[0] = NaN, r => r.runs[0].cases[0].cameraCheckpoints[1].camera.rotation_xyzw = [0, 0, 0, 0], r => r.runs[0].cases = []]) {
    const candidate = report(); candidate.cameraReference = referenceFromRuns(candidate); candidate.protocol.cameraReferenceSha256 = referenceHash(candidate.cameraReference); damage(candidate);
    assert.throws(() => validateCameraReport(candidate));
  }
});
test("recording checks trace inventory and legacy runs remain explicitly unchecked", () => {
  validateCameraReport(report());
  validateCameraReport({ protocol: {}, runs: [] });
  const candidate = report(); candidate.protocol.cameraReplay = "unknown";
  assert.throws(() => validateCameraReport(candidate));
});
test("stale samples and reference digest tampering cannot pass", () => {
  const candidate = report(); candidate.cameraReference = referenceFromRuns(candidate);
  candidate.protocol.cameraReferenceSha256 = referenceHash(candidate.cameraReference);
  candidate.runs[0].cases[0].cameraCheckpoints[1].observedAtMs = 0;
  assert.throws(() => validateCameraReport(candidate), /Stale/);
  candidate.runs[0].cases[0].cameraCheckpoints[1].observedAtMs = 100;
  candidate.cameraReference.runs[0].cases[0].cameraCheckpoints[0].camera.position[0] = 1;
  assert.throws(() => validateCameraReport(candidate), /digest/);
});
test("ordered samples outside their case time window are rejected", () => {
  const candidate = report(); candidate.runs[0].cases[0].cameraCheckpoints.forEach(p => p.observedAtMs -= 1000);
  assert.throws(() => validateCameraReport(candidate), /Stale/);
});
test("checkpoint evidence cannot silently drop its protocol marker", () => {
  const candidate = report(); delete candidate.protocol.cameraReplay;
  assert.throws(() => validateCameraReport(candidate), /protocol/);
});
test("real-input sequence names checkpoints and releases the pointer on failure", async () => {
  const { perform } = require("./perf-browser-replay.cjs");
  const points = [], pointer = [];
  const page = { mouse: { move: async () => {}, down: async () => pointer.push("down"), up: async () => pointer.push("up") }, waitForTimeout: async () => {} };
  await perform(page, "turn-return", 500, async label => points.push(label));
  assert.deepEqual(points, ["outbound-15", "outbound-30", "return-15", "return-30"]);
  assert.deepEqual(pointer, ["down", "up", "down", "up"]);
  await assert.rejects(perform(page, "turn-return", 500, async () => { throw new Error("lost telemetry"); }), /lost telemetry/);
  assert.equal(pointer.at(-1), "up");
});
test("failed camera runs retain their trace and cannot enter timing aggregation", () => {
  const { recordRun } = require("./perf-browser.cjs");
  const result = report(); result.cameraReference = referenceFromRuns(result);
  result.protocol.cameraReferenceSha256 = referenceHash(result.cameraReference);
  const run = { ...result.runs[0], errors: [], loadReadyMs: 1, rendererEncodedMs: 2, settleMs: 3,
    appIntervals: [10], rendererIntervals: [10], rafIntervals: [10],
    final: { phase: "ready", render_ready_splats: 1, renderer_encoded_frames: 1, streaming_busy: false } };
  result.runs = [];
  run.cases[0].cameraCheckpoints[2].camera.position[0] = 1;
  assert.throws(() => recordRun(result, run, {}), /Camera drift/);
  assert.equal(result.failedCameraRun, run);
  assert.deepEqual(result.runs, []);
});
test("checkpoint replay consumes pointer reposition and release before the next press", async () => {
  const { perform } = require("./perf-browser-replay.cjs");
  const events = [];
  const page = { mouse: { move: async () => events.push("move"), down: async () => events.push("down"), up: async () => events.push("up") }, waitForTimeout: async () => {} };
  await perform(page, "turn-return", 500, async () => {}, async () => events.push("frame"));
  for (let i = 0; i < events.length; i++) {
    if (events[i] === "down") assert.deepEqual(events.slice(i - 2, i), ["move", "frame"]);
    if (events[i] === "up") assert.equal(events[i + 1], "frame");
  }
});
