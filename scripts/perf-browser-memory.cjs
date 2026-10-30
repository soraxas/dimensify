// Keep this function self-contained: Playwright serializes it into the page.
function browserSnapshot() {
  const value = globalThis.__DIMENSIFY_PERF__;
  let memory;
  try { memory = globalThis.performance?.memory; } catch { /* optional API */ }
  const bytes = (key) => {
    try {
      const value = memory?.[key];
      return Number.isSafeInteger(value) && value > 0 ? value : null;
    } catch { return null; }
  };
  const used = bytes("usedJSHeapSize"), total = bytes("totalJSHeapSize"), limit = bytes("jsHeapSizeLimit");
  return { ...value, browser_memory: {
    observed_at_ms: typeof globalThis.performance?.now === "function"
      ? globalThis.performance.timeOrigin + globalThis.performance.now() : null,
    main_js_heap_used_bytes: used, main_js_heap_total_bytes: total, main_js_heap_limit_bytes: limit,
    method: "performance.memory; nonstandard approximate main-context V8 JavaScript heap; may share a heap with other contexts; excludes worker heaps, WASM capacity and browser/process RSS",
    unavailable_reason: used === null || total === null || limit === null
      ? "one or more performance.memory counters are unavailable or invalid; missing counters remain null" : null,
  } };
}
function memoryObservation(value) {
  const positive = (value) => Number.isSafeInteger(value) && value > 0 ? value : null;
  return {
    jsHeapObservedAtMs: Number.isFinite(value.browser_memory?.observed_at_ms) ? value.browser_memory.observed_at_ms : null,
    mainWasmLinearMemoryBytes: positive(value.main_wasm_linear_memory_bytes),
    mainJsHeapUsedBytes: positive(value.browser_memory?.main_js_heap_used_bytes),
    mainJsHeapTotalBytes: positive(value.browser_memory?.main_js_heap_total_bytes),
    mainJsHeapLimitBytes: positive(value.browser_memory?.main_js_heap_limit_bytes),
    workerPeakWasmLinearMemoryBytes: positive(value.worker?.peakWasmLinearMemoryBytes),
    worker: value.worker ?? null,
  };
}
module.exports = { browserSnapshot, memoryObservation };
