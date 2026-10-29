const test = require("node:test");
const assert = require("node:assert/strict");
const vm = require("node:vm");
const { browserSnapshot, memoryObservation } = require("./perf-browser-memory.cjs");
const snapshot = (performance) => vm.runInNewContext(`(${browserSnapshot.toString()})()`, {
  performance, __DIMENSIFY_PERF__: { phase: "ready", main_wasm_linear_memory_bytes: 65536 },
});
test("memory capability absence and throwing accessors preserve null counters", () => {
  for (const performance of [undefined, {}, { get memory() { throw Error("unsupported"); } }]) {
    const value = snapshot(performance);
    assert.equal(value.browser_memory.main_js_heap_used_bytes, null);
    assert.match(value.browser_memory.unavailable_reason, /unavailable/);
    assert.equal(memoryObservation(value).workerPeakWasmLinearMemoryBytes, null);
    assert.equal(value.phase, "ready");
  }
});
test("approximate JS heap observations remain distinct from WASM capacity", () => {
  const value = snapshot({ memory: { usedJSHeapSize: 1000, totalJSHeapSize: 2000, jsHeapSizeLimit: 4000 } });
  const observed = memoryObservation(value);
  assert.equal(observed.mainJsHeapUsedBytes, 1000);
  assert.equal(observed.mainJsHeapTotalBytes, 2000);
  assert.equal(observed.mainWasmLinearMemoryBytes, 65536);
  assert.match(value.browser_memory.method, /nonstandard approximate/);
  assert.equal(value.browser_memory.unavailable_reason, null);
});
test("malformed counters never become zero or a plausible byte measurement", () => {
  for (const usedJSHeapSize of [-1, NaN, Infinity, "123", 0, 2.5]) {
    const value = snapshot({ memory: { usedJSHeapSize, totalJSHeapSize: 2000 } });
    assert.equal(memoryObservation(value).mainJsHeapUsedBytes, null);
    assert.equal(memoryObservation(value).mainJsHeapTotalBytes, 2000);
    assert.equal(memoryObservation(value).mainJsHeapLimitBytes, null);
    assert.match(value.browser_memory.unavailable_reason, /unavailable/);
  }
});
