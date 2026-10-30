const { test } = require("node:test");
const assert = require("node:assert/strict");
const { summarize, validateMetadata, validateRun, metrics, isSoftwareAdapter, performancePublicationEligible, intervalSamples } = require("./perf-browser-results.cjs");
const metadata = { sourceState: "modified", commit: "a".repeat(40), profile: "release", buildIdentity: "sha256:served-wasm", hardware: "test host", asset: { identity: "sha256:fixture" }, browser: "Chromium 1", viewport: { width: 1000, height: 760 }, adapter: { name: "Apple M1", device_type: "IntegratedGpu" } };
const run = { mode: "cold", loadReadyMs: 12, rendererEncodedMs: 20, settleMs: 32, appIntervals: [10, 20, 30, 40], rendererIntervals: [20, 30], rafIntervals: [16, 17], final: { phase: "ready", renderer_encoded_frames: 2, render_ready_splats: 8, streaming_busy: false }, errors: [] };
test("nearest rank quantiles include tail outliers", () => { assert.deepEqual(summarize([40, 10, 30, 20]), { count: 4, median: 25, p95: 40, p99: 40 }); });
test("missing, nonfinite and nonpositive intervals are rejected", () => { for (const input of [[], [NaN], [Infinity], [0], [-1]]) assert.throws(() => summarize(input)); });
test("metadata must identify served release artifact and exact commit", () => { validateMetadata(metadata); for (const field of ["commit", "profile", "buildIdentity", "hardware", "asset", "adapter"]) assert.throws(() => validateMetadata({ ...metadata, [field]: null })); assert.throws(() => validateMetadata({ ...metadata, profile: "debug" })); assert.throws(() => validateMetadata({ ...metadata, commit: "main" })); });
test("software adapters cannot receive performance publication eligibility", () => { assert.equal(isSoftwareAdapter({ device: "SwiftShader Device" }), true); assert.equal(isSoftwareAdapter(metadata.adapter), false); assert.throws(() => validateMetadata({ ...metadata, adapter: { device: "llvmpipe" } }, true)); });
test("UI ready without real encoded renderer work fails", () => { assert.throws(() => validateRun({ ...run, final: { ...run.final, renderer_encoded_frames: 0 } })); });
test("busy and failed streams and browser errors fail", () => { for (const final of [{ ...run.final, streaming_busy: true }, { ...run.final, phase: "failed" }, { ...run.final, render_ready_splats: 0 }]) assert.throws(() => validateRun({ ...run, final })); assert.throws(() => validateRun({ ...run, errors: ["device lost"] })); });
test("BMF labels cadence and readiness precisely", () => { const result = metrics([run]); assert.equal(result["browser-cold-app-interval-p99-v1"].latency.value, 40_000_000); assert.equal(result["browser-cold-renderer-encoded-ready-v1"].latency.value, 20_000_000); assert.equal(Object.keys(result).some((key) => key.includes("gpu")), false); });
test("warm and cold observations remain separate", () => { const result = metrics([run, { ...run, mode: "warm" }]); assert.ok(result["browser-warm-app-interval-median-v1"]); });

const { validateReplay, defaultReplay } = require("./perf-browser-replay.cjs");
test("custom replays reject unknown actions and duplicate names", () => { validateReplay(defaultReplay); assert.throws(() => validateReplay([{ name: "main", action: "teleport" }])); assert.throws(() => validateReplay([defaultReplay[0], defaultReplay[0]])); });
test("warm replay never publishes invented reload or first-frame timing", () => { const result = metrics([{ ...run, mode: "warm", loadReadyMs: null, rendererEncodedMs: null }]); assert.equal(result["browser-warm-load-ready-v1"], undefined); assert.equal(result["browser-warm-renderer-encoded-ready-v1"], undefined); });

test("rounds contribute equally to cadence percentile aggregation", () => { const result = metrics([run, { ...run, appIntervals: [100] }]); assert.equal(result["browser-cold-app-interval-p95-v1"].latency.value, 70_000_000); });

test("static and moving cases have separate BMF identities", () => { const result = metrics([{ ...run, cases: [{ name: "static-main", appIntervals: [10, 20], rendererIntervals: [11, 21] }, { name: "moving-main", appIntervals: [30], rendererIntervals: [31] }] }]); assert.equal(result["browser-cold-moving-main-renderer-encoded-interval-p99-v1"].latency.value, 31_000_000); });
test("source state cannot silently claim a clean commit", () => { assert.throws(() => validateMetadata({ ...metadata, sourceState: undefined })); });

test("modified builds stay diagnostic even on hardware adapters", () => { assert.equal(performancePublicationEligible(metadata), false); assert.equal(performancePublicationEligible({ ...metadata, sourceState: "clean" }), true); assert.throws(() => validateMetadata(metadata, true)); });
test("worker profiling and remote-device isolation cannot become frame performance evidence",()=>{
  for (const extra of [{workerProfiling:true},{physicalDevice:{kind:'physical-mobile'}}])
    assert.equal(performancePublicationEligible({...metadata,sourceState:'clean',...extra}),false);
});

test("BMF latency converts raw milliseconds to nanoseconds", () => { const result = metrics([run]); assert.equal(result["browser-cold-app-interval-median-v1"].latency.value, 25_000_000); });
test("CPU device types fail publication adapter check", () => { assert.equal(isSoftwareAdapter({ name: "Adapter", device_type: "Cpu" }), true); });

const { prepareOutput } = require("./perf-browser-output.cjs");
const fs = require("node:fs"), os = require("node:os"), path = require("node:path");
test("failed reruns cannot leave old BMF results beside new raw data", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-browser-output-"));
  try { prepareOutput(directory); fs.writeFileSync(path.join(directory, "bmf.json"), "{}"); assert.throws(() => prepareOutput(directory)); assert.equal(fs.readFileSync(path.join(directory, "bmf.json"), "utf8"), "{}"); }
  finally { fs.rmSync(directory, { recursive: true }); }
});

test("per-case RAF and app extraction rejects silently truncated histories", () => {
  assert.deepEqual(intervalSamples({ total: 4, intervals: [[2, 10, 10], [3, 20, 10], [4, 30, 10]] }, 9, 25), [10, 10]);
  assert.throws(() => intervalSamples({ total: 1000, intervals: [[999, 5000, 16], [1000, 5016, 16]] }, 0, 5020));
});

test("browser GPU inventory cannot attest a redacted active WebGPU adapter", () => { const m = { ...metadata, sourceState: "clean", adapter: { name: "", device_type: "Other", backend: "BrowserWebGpu", browser_device: "Apple M5 Max", evidence_scope: "inventory only" } }; assert.equal(performancePublicationEligible(m), false); assert.throws(() => validateMetadata(m, true)); });

test("camera actions retain observations while work is pending", async () => {
  const { observeDuring } = require("./perf-browser-replay.cjs");
  let finish, count = 0;
  const work = new Promise((resolve) => { finish = resolve; });
  await observeDuring(() => work, async () => { count++; if (count === 3) finish(); }, 1);
  assert.ok(count >= 3);
});

test("observation failure waits for input cleanup and rejects the case", async () => {
  const { observeDuring } = require("./perf-browser-replay.cjs");
  let cleaned = false;
  await assert.rejects(observeDuring(async () => { await new Promise((resolve) => setTimeout(resolve, 5)); cleaned = true; }, async () => { throw new Error("device lost"); }, 1), /device lost/);
  assert.equal(cleaned, true);
});

test("falsy work and observation rejections still reject after cleanup", async () => {
  const { observeDuring } = require("./perf-browser-replay.cjs");
  for (const reason of [undefined, null, false, 0, ""]) {
    await assert.rejects(observeDuring(() => Promise.reject(reason), async () => {}, 1), (error) => error === reason);
    let cleaned = false;
    await assert.rejects(observeDuring(async () => {
      await new Promise((resolve) => setTimeout(resolve, 5));
      cleaned = true;
    }, () => Promise.reject(reason), 1), (error) => error === reason);
    assert.equal(cleaned, true);
  }
});

test("one GPU lifetime report is retained on the final warm run", () => {
  const { recordRun } = require("./perf-browser.cjs");
  const { createGpuCollector } = require("./perf-browser-gpu.cjs");
  const gpu = createGpuCollector(), result = { runs: [] };
  const observation = (completed) => ({ gpu: { passes: [{ scope: "raster", capability: "timestamp_query", completed, samples: Array.from({ length: completed }, (_, i) => ({ identity: { selection: String(i + 1), frame: String(i + 1) }, measurement: { duration_ms: 1 } })) }] } });
  gpu.observe(observation(1));
  recordRun(result, { ...run }, gpu);
  gpu.observe(observation(2));
  recordRun(result, { ...run, mode: "warm" }, gpu);
  const retained = JSON.parse(JSON.stringify(result)).runs;
  assert.equal(Object.hasOwn(retained[0], "gpuSamples"), false);
  assert.equal(retained[1].gpuSamples.samples.length, 2);
});
