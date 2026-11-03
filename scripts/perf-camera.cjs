// Camera checkpoint assertions only; never drives or teleports the viewer.
const { isDeepStrictEqual } = require("node:util");
const crypto = require("node:crypto");
const VERSION = "input-checkpoints-v1";
const SCHEMA = "dimensify-camera-reference/1";
const tolerance = { positionWorldUnits: 1e-4, rotationRadians: 1e-4, projectionRelative: 1e-6 };
function requireValue(ok, message) { if (!ok) throw new Error(message); }
function labels(action) {
  if (["hold", "frame"].includes(action)) return ["before", "after"];
  if (action === "near-side") return ["before", "outbound-15", "outbound-30", "zoom", "after"];
  requireValue(["turn-return", "pan-return"].includes(action), "Unknown camera replay action");
  return ["before", "outbound-15", "outbound-30", "return-15", "return-30", "after"];
}
function validatePose(pose) {
  for (const [key, length] of [["position", 3], ["rotation_xyzw", 4]])
    requireValue(Array.isArray(pose?.[key]) && pose[key].length === length && pose[key].every(Number.isFinite), "Invalid camera pose");
  requireValue(Math.abs(Math.hypot(...pose.rotation_xyzw) - 1) < .001, "Invalid camera rotation");
  requireValue(Number.isFinite(pose.fov_radians) && pose.fov_radians > 0 && pose.fov_radians < Math.PI && Number.isFinite(pose.near) && pose.near > 0 && Number.isFinite(pose.far) && pose.far > pose.near, "Invalid camera projection");
}
function identity(report) {
  const environment = report.metadata?.browserEnvironment;
  const value = { asset: report.metadata?.asset?.identity, viewport: report.metadata?.viewport,
    renderSurface: { dpr: environment?.dpr, canvas: environment?.canvas },
    durationMs: report.protocol?.durationMs, steps: report.protocol?.steps };
  requireValue(Number.isFinite(value.renderSurface.dpr) && value.renderSurface.dpr > 0 && Number.isSafeInteger(value.renderSurface.canvas?.width) && value.renderSurface.canvas.width > 0 && Number.isSafeInteger(value.renderSurface.canvas?.height) && value.renderSurface.canvas.height > 0, "Invalid camera render surface identity");
  requireValue(typeof value.asset === "string" && value.asset.length > 0 && Number.isSafeInteger(value.viewport?.width) && value.viewport.width > 0 && Number.isSafeInteger(value.viewport?.height) && value.viewport.height > 0 && Number.isFinite(value.durationMs) && value.durationMs >= 500, "Invalid camera replay identity");
  requireValue(Array.isArray(value.steps) && value.steps.length > 0 && value.steps.length <= 32 && new Set(value.steps.map(s => s.name)).size === value.steps.length, "Invalid camera replay inventory");
  for (const step of value.steps) { requireValue(typeof step.name === "string", "Invalid camera case"); labels(step.action); }
  return structuredClone(value);
}
function validateCases(cases, expected) {
  requireValue(Array.isArray(cases) && isDeepStrictEqual(cases.map(c => c.name), expected.map(s => s.name)), "Camera case inventory mismatch");
  let previousCaseEnd = -Infinity;
  for (let i = 0; i < cases.length; i++) {
    const item = cases[i];
    requireValue(Number.isFinite(item.began) && Number.isFinite(item.ended) && item.ended >= item.began, "Invalid camera case time window");
    requireValue(item.began >= previousCaseEnd, "Camera case chronology mismatch");
    previousCaseEnd = item.ended;
    requireValue(item.settings && typeof item.settings === "object" && !Array.isArray(item.settings), "Missing camera case settings");
    requireValue(Array.isArray(item.cameraCheckpoints) && isDeepStrictEqual(item.cameraCheckpoints.map(p => p.label), labels(expected[i].action)), "Camera checkpoint inventory mismatch");
    let previous = -Infinity;
    for (const point of item.cameraCheckpoints) {
      requireValue(Number.isFinite(point.observedAtMs) && point.observedAtMs > previous && point.observedAtMs >= item.began && point.observedAtMs <= item.ended, "Stale camera checkpoint telemetry");
      previous = point.observedAtMs;
      validatePose(point.camera);
    }
  }
}
function referenceFromRuns(report) {
  const context = identity(report);
  const runs = ["cold", "warm"].map(mode => {
    const run = report.runs.find(r => r.mode === mode);
    requireValue(run, "Missing camera reference mode");
    validateCases(run.cases, context.steps);
    return { mode, cases: run.cases.map(({ name, settings, cameraCheckpoints, began, ended }) => ({ name, settings, cameraCheckpoints, began, ended })) };
  });
  return structuredClone({ schema: SCHEMA, identity: context, tolerance, runs });
}
function referenceHash(reference) { return crypto.createHash("sha256").update(JSON.stringify(reference)).digest("hex"); }
function validateCameraReport(report) {
  const marker = report.protocol?.cameraReplay;
  if (marker === undefined) {
    requireValue(!report.cameraReference && !report.protocol?.cameraReferenceSha256 && !report.runs?.some(run => run.cases?.some(item => item.cameraCheckpoints !== undefined)), "Camera reference requires checkpoint protocol");
    return;
  }
  requireValue(marker === VERSION, "Unknown camera checkpoint protocol");
  const context = identity(report), reference = report.cameraReference;
  requireValue(Array.isArray(report.runs) && report.runs.length > 0, "Missing camera runs");
  if (reference) {
    requireValue(reference.schema === SCHEMA && isDeepStrictEqual(reference.identity, context) && isDeepStrictEqual(reference.tolerance, tolerance), "Camera reference identity mismatch");
    requireValue(Array.isArray(reference.runs) && isDeepStrictEqual(reference.runs.map(r => r.mode), ["cold", "warm"]), "Camera reference mode inventory mismatch");
    reference.runs.forEach(r => validateCases(r.cases, context.steps));
    requireValue(report.protocol.cameraReferenceSha256 === referenceHash(reference), "Camera reference digest mismatch");
  } else requireValue(report.protocol.cameraReferenceSha256 == null, "Missing camera reference");
  for (const run of report.runs) {
    requireValue(["cold", "warm"].includes(run.mode), "Unknown camera mode");
    validateCases(run.cases, context.steps);
    if (!reference) continue;
    const expected = reference.runs.find(r => r.mode === run.mode);
    for (let i = 0; i < run.cases.length; i++) {
      const actual = run.cases[i], base = expected.cases[i];
      requireValue(isDeepStrictEqual(actual.settings, base.settings), "Camera case settings mismatch");
      for (let j = 0; j < actual.cameraCheckpoints.length; j++) {
        const a = actual.cameraCheckpoints[j].camera, b = base.cameraCheckpoints[j].camera;
        const position = Math.hypot(...a.position.map((v, k) => v - b.position[k]));
        const dot = Math.abs(a.rotation_xyzw.reduce((sum, v, k) => sum + v * b.rotation_xyzw[k], 0) / (Math.hypot(...a.rotation_xyzw) * Math.hypot(...b.rotation_xyzw)));
        const angle = 2 * Math.acos(Math.min(1, dot));
        const projection = ["fov_radians", "near", "far"].every(key => Math.abs(a[key] - b[key]) <= tolerance.projectionRelative * Math.max(1, Math.abs(b[key])));
        requireValue(position <= tolerance.positionWorldUnits && angle <= tolerance.rotationRadians && projection, `Camera drift: ${run.mode}/${actual.name}/${actual.cameraCheckpoints[j].label}`);
      }
    }
  }
}
module.exports = { VERSION, tolerance, labels, referenceFromRuns, referenceHash, validateCameraReport };
