#!/usr/bin/env node
// Opt-in production-viewer replay. No publication or credentials are used.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { browserSnapshot, memoryObservation } = require("./perf-browser-memory.cjs");
const { createGpuCollector } = require("./perf-browser-gpu.cjs");
const { startWorkerProfiling } = require("./perf-browser-cpu.cjs");
const { connectionOptions, connectBrowser, observeDevice, validateReplayViewport, sanitize } = require("./perf-device.cjs");
const { prepareOutput } = require("./perf-browser-output.cjs");
const { parseArgs } = require("node:util");
const { summarize, validateMetadata, validateRun, metrics, performancePublicationEligible, intervalSamples } = require("./perf-browser-results.cjs");
const { defaultReplay, validateReplay, perform, frameHome, observeDuring } = require("./perf-browser-replay.cjs");

async function main() {
  const { values: args } = parseArgs({ options: {
    url: { type: "string" }, file: { type: "string" }, "asset-url": { type: "string" }, "asset-id": { type: "string" },
    commit: { type: "string" }, "source-state": { type: "string" }, "build-id": { type: "string" }, hardware: { type: "string" },
    profile: { type: "string", default: "release" }, out: { type: "string", default: "/tmp/dimensify-browser-perf" },
    rounds: { type: "string", default: "3" }, "duration-ms": { type: "string", default: "2000" },
    "timeout-ms": { type: "string", default: "180000" }, replay: { type: "string" },
    "splat-budget": { type: "string" }, "cache-mib": { type: "string" }, "source-mib": { type: "string" }, concurrency: { type: "string" },
    "publication-check": { type: "boolean", default: false }, headed: { type: "boolean", default: false },
    "worker-profile": { type: "boolean", default: false }, target: { type: "string" },
    "cdp-endpoint": { type: "string" }, "ws-endpoint": { type: "string" },
  } });
  if (!args.url || Boolean(args.file) === Boolean(args["asset-url"]) || !args.commit || !args["source-state"] || !args["build-id"] || !args.hardware || (args["asset-url"] && !args["asset-id"])) throw new Error("Usage: node scripts/perf-browser.cjs --url <served release viewer> (--file scene.ply | --asset-url hierarchy.json --asset-id sha256:...) --commit <40 hex served base commit> --source-state clean|modified --build-id <served WASM digest> --hardware <host description> [--out directory] [--rounds 3] [--publication-check]");
  const rounds = Number(args.rounds), durationMs = Number(args["duration-ms"]), timeoutMs = Number(args["timeout-ms"]);
  if (!Number.isInteger(rounds) || rounds < 1 || rounds > 20 || !Number.isFinite(durationMs) || durationMs < 500 || durationMs > 60000 || !Number.isFinite(timeoutMs) || timeoutMs < 1000 || timeoutMs > 600000) throw new Error("Invalid replay duration/round/timeout limits");
  const steps = validateReplay(args.replay ? JSON.parse(fs.readFileSync(args.replay, "utf8")) : defaultReplay);
  const output = prepareOutput(args.out);
  const asset = args.file ? { name: path.basename(args.file), identity: `sha256:${crypto.createHash("sha256").update(fs.readFileSync(args.file)).digest("hex")}`, bytes: fs.statSync(args.file).size, input: "local-file-picker" } : { identity: args["asset-id"], url: args["asset-url"], input: "url-panel" };
  const url = new URL(args.url); url.searchParams.set("perf", "1");
  for (const [option, parameter, max] of [["splat-budget", "perf_splat_budget", 16000000], ["cache-mib", "perf_cache_mib", 2048], ["source-mib", "perf_source_mib", 16384], ["concurrency", "perf_concurrency", 32]]) {
    if (args[option] === undefined) continue;
    const value = Number(args[option]);
    if (!Number.isInteger(value) || value < 1 || value > max) throw new Error(`Invalid ${option}: expected 1..${max}`);
    url.searchParams.set(parameter, String(value));
  }
  const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
  const remote = connectionOptions(args);
  const result = { schema: "dimensify-browser-replay/1", startedAt: new Date().toISOString(), protocol: { sampling: "production continuous renderer; no single-flight completion; partial scoped GPU samples reported separately", quantiles: "median averages middle pair; P95/P99 nearest rank per run; median of run percentiles for each mode", cold: "fresh browser context, empty HTTP cache and new viewer/renderer", warm: "same loaded scene and context; repeat camera replay with live application caches, no second load", intervalUnit: "milliseconds", bmfLatencyUnit: "nanoseconds", settleWindowMs: 1000, durationMs, steps, telemetryPublishMs: 100, instrumentation: "perf=1; bounded frame and source-event histories; snapshots every 100ms" }, metadata: { sourceState: args["source-state"], instrumentedBuildFeature: "splat-viewer-perf", commit: args.commit, profile: args.profile, buildIdentity: args["build-id"], hardware: args.hardware, asset, viewport: { width: 1000, height: 760 }, viewerUrl: url.href, platform: process.platform, arch: process.arch }, runs: [], errors: [] };
  result.protocol.observationSampling = "v4: bounded originating-frame GPU reports, scoped buffer capacities, WASM/worker and approximate JS heap sampled at 100ms; no total VRAM or full GPU frame duration";
  result.protocol.workerProfiling = args["worker-profile"] ? "V8 sampled worker profiles; paused startup and delayed termination; diagnostic only" : "disabled";
  result.metadata.workerProfiling = args["worker-profile"];
  result.metadata.physicalDevice = remote.target;
  if (remote.target) result.metadata.hardware = `${remote.target.model}/${remote.target.os}/${remote.target.id}`;
  result.workerProfiles = [];
  // A new process per round also isolates browser GPU/device startup and HTTP cache.
  for (let round = 1; round <= rounds; round++) {
    const connection = await connectBrowser(chromium, { ...remote, timeoutMs,
      launchOptions: {headless: !args.headed, executablePath: process.env.CHROMIUM_EXECUTABLE, args: ["--enable-unsafe-webgpu", ...(process.platform === "darwin" ? ["--use-angle=metal"] : ["--enable-features=Vulkan", "--disable-vulkan-surface"])]},
      contextOptions: {viewport: result.metadata.viewport, deviceScaleFactor: 1} });
    const {browser,context} = connection;
    result.protocol.connection = connection.connection;
    result.protocol.isolation = connection.isolation;
    if (remote.target) result.protocol.cold = "initial load on physical device; browser process/GPU startup isolation and HTTP cache state are not guaranteed";
    let page, profiler;
    const gpu = createGpuCollector();
    const errors = [], logs = [];
    try {
      page = await context.newPage();
      if (args["worker-profile"]) profiler = await startWorkerProfiling({page,context});
      page.on("pageerror", (error) => errors.push(String(error)));
      page.on("console", (message) => { logs.push(message.text()); if (message.type() === "error" || message.text().includes("%cERROR%c")) errors.push(message.text()); });
      await page.addInitScript(() => {
        const history = { total: 0, intervals: [] }; let previous;
        window.__DIMENSIFY_RAF__ = history;
        document.addEventListener("change", (event) => { if (event.target?.id === "dimensify-file-input") window.__DIMENSIFY_INPUT_AT__ = performance.timeOrigin + performance.now(); }, true);
        document.addEventListener("keydown", (event) => { if (event.key === "Enter") window.__DIMENSIFY_INPUT_AT__ = performance.timeOrigin + performance.now(); }, true);
        const tick = (now) => { if (previous !== undefined) { if (history.intervals.length === 4096) history.intervals.shift(); history.intervals.push([++history.total, performance.timeOrigin + now, now - previous]); } previous = now; requestAnimationFrame(tick); };
        requestAnimationFrame(tick);
      });
      await page.goto(url.href);
      result.metadata.observedDevice = await observeDevice(page, remote.target);
      validateReplayViewport(result.metadata.observedDevice, remote.target);
      await page.waitForFunction(() => window.__DIMENSIFY_PERF__?.schema === "dimensify-browser-telemetry/1", null, { timeout: timeoutMs });
      await page.locator("#dimensify-file-input").waitFor({ state: "attached", timeout: timeoutMs });
      await page.waitForTimeout(500);
      result.metadata.browser = browser.version();
      let systemInfo = {gpu:{devices:[],auxAttributes:{}}};
      try {
        const browserSession = await browser.newBrowserCDPSession();
        try { systemInfo = await browserSession.send("SystemInfo.getInfo"); } finally { await browserSession.detach(); }
      } catch { result.metadata.browserGpuInventoryUnavailableReason = "Browser-level SystemInfo unavailable on this connection; no hardware attestation inferred"; }
      result.metadata.browserGpuInfo = systemInfo.gpu;
      const empty = await page.evaluate(() => ({ telemetry: window.__DIMENSIFY_PERF__, dpr: devicePixelRatio, canvas: { width: document.querySelector("canvas").width, height: document.querySelector("canvas").height }, userAgent: navigator.userAgent }));
      result.metadata.adapter = { ...empty.telemetry.adapter,
        browser_device: systemInfo.gpu.devices.map((device) => device.deviceString || device.device_string || "").filter(Boolean).join("; "),
        browser_gl_renderer: String(systemInfo.gpu.auxAttributes?.glRenderer || ""),
        evidence_scope: "Bevy active adapter (may be privacy redacted) plus browser process GPU device/GL renderer inventory; browser inventory is not a WebGPU device attestation",
      };
      result.metadata.browserEnvironment = { dpr: empty.dpr, canvas: empty.canvas, userAgent: empty.userAgent };
      if (remote.target) result.metadata.viewport = {width:result.metadata.observedDevice.inner.width,height:result.metadata.observedDevice.inner.height};
      validateMetadata(result.metadata, args["publication-check"]);
      result.performancePublicationEligible = performancePublicationEligible(result.metadata);
      await page.screenshot({ path: path.join(output, `round-${round}-empty.png`) });
      let inputStart;
      if (args.file) {
        const chooser = page.waitForEvent("filechooser", { timeout: 10000 });
        await page.mouse.click(330, 46, { delay: 120 });
        const selected = await chooser;
        inputStart = await page.evaluate(() => Date.now());
        await selected.setFiles(path.resolve(args.file));
        inputStart = await page.evaluate(() => window.__DIMENSIFY_INPUT_AT__);
      } else {
        await page.mouse.click(82, 90, { delay: 120 });
        await page.waitForTimeout(300);
        await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A", { delay: 120 });
        await page.waitForTimeout(300);
        await page.keyboard.type(args["asset-url"], { delay: 3 });
        await page.waitForTimeout(300);
        inputStart = await page.evaluate(() => Date.now());
        await page.keyboard.press("Enter", { delay: 120 });
        inputStart = await page.evaluate(() => window.__DIMENSIFY_INPUT_AT__);
      }
      if (!Number.isFinite(inputStart)) throw new Error("Missing real browser load input event timestamp");
      let loadReadyMs, rendererEncodedMs;
      const observations = [];
      const snapshot = async () => {
        if (errors.length) throw new Error(errors.join("\n"));
        const value = await page.evaluate(browserSnapshot);
        gpu.observe(value);
        if (value.phase === "failed" || value.notice?.startsWith("Could not")) throw new Error(`Viewer failed: ${value.name} ${value.notice || ""}`);
        if (value.phase === "ready" && loadReadyMs === undefined) loadReadyMs = Math.max(0, value.phase_changed_at_ms - inputStart);
        const first = value.renderer_frames.first_at_ms;
        if (first >= inputStart && first !== null && rendererEncodedMs === undefined) rendererEncodedMs = first - inputStart;
        if (observations.length === 20000) throw new Error("Observation cap exceeded");
        observations.push({ observedAtMs: value.observed_at_ms, phase: value.phase, renderReadySplats: value.render_ready_splats, busy: value.streaming_busy, pendingDownloads: value.pending_downloads, pendingGenerations: value.pending_generations, encodedFrames: value.renderer_encoded_frames, cacheReadyBytes: value.cache_ready_bytes, cacheReservedBytes: value.cache_reserved_bytes, gpuBuffers:value.gpu_buffers ?? null, ...memoryObservation(value) });
        return value;
      };
      const settle = async () => {
        const deadline = Date.now() + timeoutMs; let stableStart, signature;
        while (Date.now() < deadline) {
          const value = await snapshot();
          const current = `${value.render_ready_splats}:${value.pending_downloads}:${value.pending_generations}`;
          if (value.phase === "ready" && value.render_ready_splats > 0 && value.renderer_encoded_frames > 0 && !value.streaming_busy && !value.notice) {
            if (signature !== current) { signature = current; stableStart = value.observed_at_ms; }
            if (value.observed_at_ms - stableStart >= 1000) return { value, settledAtMs: stableStart };
          } else { signature = undefined; stableStart = undefined; }
          await page.waitForTimeout(50);
        }
        throw new Error("Viewer never reached stable render-ready selection with encoded draw+sort work");
      };
      const initial = await settle();
      await page.mouse.click(698, 46, { delay: 120 }); // Default Fly -> Orbit.
      result.protocol.navigationMode = "orbit";
      await frameHome(page);
      await settle();
      for (const mode of ["cold", "warm"]) {
        // Warm starts from the same framed camera and retains live source caches.
        await frameHome(page);
        await settle();
        const start = await page.evaluate(() => Date.now());
        const cases = [];
        for (const step of steps) {
          const began = await page.evaluate(() => Date.now());
          const before = await snapshot();
          await observeDuring(() => perform(page, step.action, durationMs), snapshot);
          const after = await snapshot();
          const ended = await page.evaluate(() => Date.now());
          const rafHistory = await page.evaluate(() => window.__DIMENSIFY_RAF__);
          const appIntervals = intervalSamples(after.app_frames, began, ended);
          const rendererIntervals = intervalSamples(after.renderer_frames, began, ended);
          const rafIntervals = intervalSamples(rafHistory, began, ended);
          if (!appIntervals.length || !rendererIntervals.length) throw new Error(`No renderer/app intervals for ${step.name}`);
          cases.push({ ...step, began, ended, appIntervals, rendererIntervals, rafIntervals, browserRaf: summarize(rafIntervals), app: summarize(appIntervals), rendererEncoded: summarize(rendererIntervals), cameraBefore: before.camera, cameraAfter: after.camera, settings: after.settings, renderReadySplats: after.render_ready_splats, cacheReadyBytes: after.cache_ready_bytes, cacheReservedBytes: after.cache_reserved_bytes });
          await page.screenshot({ path: path.join(output, `round-${round}-${mode}-${step.name}.png`) });
        }
        const settled = await settle();
        const rafIntervals = cases.flatMap((item) => item.rafIntervals);
        const run = { round, mode, adapter: result.metadata.adapter, browser: result.metadata.browser, browserEnvironment: result.metadata.browserEnvironment, inputStartMs: mode === "cold" ? inputStart : null, replayStartMs: start, loadReadyMs: mode === "cold" ? loadReadyMs : null, loadReadyScope: mode === "cold" ? "file selection/URL submit to main-world loader Ready" : "already loaded; no load measured", rendererEncodedMs: mode === "cold" ? rendererEncodedMs : null, settleMs: mode === "cold" ? initial.settledAtMs - inputStart : settled.settledAtMs - start, appIntervals: cases.flatMap((x) => x.appIntervals), rendererIntervals: cases.flatMap((x) => x.rendererIntervals), rafIntervals, cases, final: settled.value, observations: [...observations], errors: [...errors], visiblePixelsMs: null, visiblePixelsUnavailableReason: "screenshots retained for review; no timestamped pixel-detection protocol" };
        run.gpuSamples = gpu.report();
        validateRun(run); result.runs.push(run);
      }
    } catch (error) {
      result.errors.push({ round, message: sanitize(error,[remote.cdpEndpoint,remote.wsEndpoint]), browserErrors: errors, lastTelemetry: page ? await page.evaluate(() => window.__DIMENSIFY_PERF__).catch(() => null) : null });
      if (page) await page.screenshot({ path: path.join(output, `round-${round}-failure.png`) }).catch(() => {});
      throw new Error(sanitize(error,[remote.cdpEndpoint,remote.wsEndpoint]));
    } finally {
      if (profiler) result.workerProfiles.push({round,...await profiler.stop()});
      fs.writeFileSync(path.join(output, `round-${round}-browser.log`), logs.join("\n"));
      fs.writeFileSync(path.join(output, "raw.json"), JSON.stringify(result, null, 2));
      if (remote.target && page) await page.close().catch(()=>{});
      await connection.close();
    }
  }
  fs.writeFileSync(path.join(output, "bmf.json"), JSON.stringify(args["worker-profile"] ? {} : metrics(result.runs), null, 2));
  console.log(JSON.stringify({ result: "PASS", rounds, output, adapter: result.metadata.adapter, performancePublicationEligible: result.performancePublicationEligible, note: "scoped GPU samples and capacity observations; no full GPU frame, display FPS or resident VRAM measurement" }));
}
main().catch((error) => { console.error(sanitize(error,[process.env.DIMENSIFY_CDP_ENDPOINT,process.env.BROWSERSTACK_WS_ENDPOINT])); process.exitCode = 1; });
