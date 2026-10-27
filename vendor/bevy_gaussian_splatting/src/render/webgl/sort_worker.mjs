// Persistent engine-independent WASM sorter. Only positions enter; only indices leave.
const sorters = new Map();
let module;
async function boot() {
  const glue = new URL("./splat-sort/splat_sort.js", self.location.href);
  const wasm = new URL("./splat-sort/splat_sort_bg.wasm", self.location.href);
  glue.search = wasm.search = self.location.search;
  const exports = await import(glue.href);
  await exports.default({ module_or_path: wasm });
  return exports;
}

// Serialize deletion behind an in-flight async boot; a retired source must not
// be recreated by a continuation after its remove message was received.
let chain = Promise.resolve();
self.onmessage = ({ data }) => {
  chain = chain.then(async () => {
    if (data.remove !== undefined) {
      sorters.get(data.remove)?.free();
      sorters.delete(data.remove);
      return;
    }
    try {
      module ??= await boot();
      if (data.positions) {
        sorters.get(data.id)?.free();
        sorters.set(data.id, new module.DepthSorter(data.positions));
      }
      const sorter = sorters.get(data.id);
      if (!sorter) throw new Error("Unknown sort source");
      const start = performance.now();
      const order = sorter.sort(data.axis);
      self.postMessage({ id: data.id, order, sortMs: performance.now() - start }, [order.buffer]);
    } catch (error) {
      self.postMessage({ id: data.id, error: String(error) });
    }
  });
};
