// Pure contracts shared by the browser replay and its tests.
function summarize(values) {
  if (!Array.isArray(values) || !values.length || values.some((x) => !Number.isFinite(x) || x <= 0)) throw new Error("Expected positive finite interval samples");
  const sorted = [...values].sort((a, b) => a - b);
  const rank = (q) => sorted[Math.ceil(sorted.length * q) - 1];
  return { count: sorted.length, median: (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2, p95: rank(.95), p99: rank(.99) };
}
function intervalSamples(history, began, ended) {
  const entries = history.intervals;
  if (entries.length && history.total > entries.length && entries[0][1] - entries[0][2] > began) throw new Error("Frame history capacity exceeded; shorten scenario duration");
  return entries.filter((entry) => entry[1] >= began && entry[1] <= ended).map((entry) => entry[2]);
}
function isSoftwareAdapter(adapter) { return String(adapter?.device_type).toLowerCase() === "cpu" || /swiftshader|llvmpipe|lavapipe|software|microsoft basic render/i.test(JSON.stringify(adapter)); }
function validateMetadata(m, publication = false) {
  if (!m || !/^[a-f0-9]{40}$/.test(m.commit || "") || m.profile !== "release" || !["clean", "modified"].includes(m.sourceState)) throw new Error("Require exact served build commit and release profile");
  for (const key of ["buildIdentity", "hardware", "browser"]) if (typeof m[key] !== "string" || !m[key].trim()) throw new Error(`Missing metadata: ${key}`);
  if (!m.asset?.identity || !m.adapter || !Object.values(m.adapter).some((x) => typeof x === "string" && x.trim()) || !(m.viewport?.width > 0 && m.viewport?.height > 0)) throw new Error("Missing asset, adapter or viewport identity");
  if (publication && m.sourceState !== "clean") throw new Error("Modified source builds are diagnostic only");
  if (publication && (!/^(integratedgpu|discretegpu)$/i.test(m.adapter.device_type || "") || !m.adapter.name?.trim())) throw new Error("Publication requires an identified active hardware adapter; browser inventory alone is insufficient");
  if (publication && isSoftwareAdapter(m.adapter)) throw new Error("Software adapter cannot publish performance results");
}
function performancePublicationEligible(m) {
  try { validateMetadata(m, true); return true; } catch (_) { return false; }
}
function validateRun(run) {
  if (!["cold", "warm"].includes(run.mode)) throw new Error("Invalid cache mode");
  for (const key of ["loadReadyMs", "rendererEncodedMs", "settleMs"]) if (!(run.mode === "warm" && key !== "settleMs" && run[key] === null) && (!Number.isFinite(run[key]) || run[key] < 0)) throw new Error(`Invalid timing: ${key}`);
  if (run.errors.length || run.final.phase !== "ready" || run.final.streaming_busy || !(run.final.render_ready_splats > 0) || !(run.final.renderer_encoded_frames > 0)) throw new Error("Replay lacks successful settled renderer evidence");
  for (const values of [run.appIntervals, run.rendererIntervals, run.rafIntervals]) summarize(values);
}
function metrics(runs) {
  const result = {};
  for (const mode of ["cold", "warm"]) {
    const group = runs.filter((run) => run.mode === mode);
    if (!group.length) continue;
    group.forEach(validateRun);
    const add = (name, value) => { result[`browser-${mode}-${name}-v1`] = { latency: { value: value * 1_000_000 } }; };
    for (const [key, name] of [["loadReadyMs", "load-ready"], ["rendererEncodedMs", "renderer-encoded-ready"], ["settleMs", "refinement-settle"]]) {
      if (mode === "warm" && key !== "settleMs") continue;
      add(key === "settleMs" && mode === "warm" ? "replay-settle" : name, summarize(group.map((run) => Math.max(Number.EPSILON, run[key]))).median);
    }
    for (const [key, name] of [["appIntervals", "app-interval"], ["rendererIntervals", "renderer-encoded-interval"], ["rafIntervals", "browser-raf-interval"]]) {
      for (const quantile of ["median", "p95", "p99"]) add(`${name}-${quantile}`, summarize(group.map((run) => summarize(run[key])[quantile])).median);
    }
    for (const name of new Set(group.flatMap((run) => (run.cases || []).map((item) => item.name)))) {
      const cases = group.map((run) => run.cases?.find((item) => item.name === name));
      if (cases.some((item) => !item)) throw new Error(`Incomplete scenario across rounds: ${name}`);
      for (const [key, label] of [["appIntervals", "app-interval"], ["rendererIntervals", "renderer-encoded-interval"]]) {
        for (const quantile of ["median", "p95", "p99"]) add(`${name}-${label}-${quantile}`, summarize(cases.map((item) => summarize(item[key])[quantile])).median);
      }
    }
  }
  return result;
}
module.exports = { summarize, validateMetadata, validateRun, metrics, isSoftwareAdapter, performancePublicationEligible, intervalSamples };
