// Engine-independent worker. The same Rust WebP codec preserves arbitrary RGBA
// data, including colour bytes under zero alpha. No Canvas/ImageDecoder path.

// wasm-bindgen init returns the actual instance exports, including memory.
// Linear memory grows monotonically; reading after decode also observes its
// allocation high water even when Rust has already freed the decoded values.
export function wasmLinearMemoryBytes(exports) {
  try {
    const bytes = exports?.memory?.buffer?.byteLength;
    return Number.isSafeInteger(bytes) && bytes > 0 && bytes % 65536 === 0
      ? bytes : null;
  } catch { return null; }
}

// Boot at module evaluation so a pre-spawned worker is ready before its job.
// A spare that is terminated unused must not report an unhandled rejection.
let boot;
if (typeof self !== "undefined") {
  boot = (async () => {
    // Content version comes from the app's compiled codec/source fingerprint,
    // not a timestamp. Version both glue and WASM to avoid mixed cached builds.
    const glue = new URL("./sog-codec/sog_codec.js", self.location.href);
    const wasm = new URL("./sog-codec/sog_codec_bg.wasm", self.location.href);
    glue.search = wasm.search = self.location.search;
    const codec = await import(glue.href);
    return { codec, exports: await codec.default({ module_or_path: wasm }) };
  })();
  boot.catch(() => {});
}

// timeOrigin differs per context; a prewarmed worker can observe a message
// fractionally "before" it was sent. Treat sub-millisecond skew as zero.
export const CLOCK_SKEW_MS = 1;
export function crossContextInterval(from, to) {
  if (!Number.isFinite(from) || !Number.isFinite(to) || to - from < -CLOCK_SKEW_MS)
    return null;
  return Math.max(0, to - from);
}

let consumed = false;
if (typeof self !== "undefined") self.onmessage = async ({ data }) => {
  if (consumed) return; // One bounded job per worker; terminate to release WASM high water.
  consumed = true;
  const { identity, files, maxDimension, maxOutputBytes, expanded } = data;
  const started = performance.now();
  const receivedAtMs = performance.timeOrigin + started;
  let booted = false;
  let wasmExports;
  let decodeMs = null;
  const send = (result, transfer = []) => {
    const now = performance.now();
    const dispatched = data.dispatchedAtMs;
    self.postMessage({
      ...result,
      telemetry: {
        handlerElapsedMs: now - started,
        decodeMs,
        dispatchToHandlerMs: crossContextInterval(dispatched, receivedAtMs),
        sentAtMs: performance.timeOrigin + now,
        wasmLinearMemoryBytes: wasmLinearMemoryBytes(wasmExports),
      },
    }, transfer);
  };
  try {
    const { codec, exports } = await boot;
    const { decode_image, ExpandedJob } = codec;
    wasmExports = exports;
    booted = true;
    if (expanded) {
      const decodeStarted = performance.now();
      const job = new ExpandedJob(
        expanded.meta,
        expanded.coefficientsPerChannel,
      );
      let result;
      try {
        for (const file of files)
          job.add(file.name, new Uint8Array(file.bytes));
        result = job.finish();
      } finally {
        job.free();
        decodeMs = performance.now() - decodeStarted;
      }
      // wasm-bindgen's result owns a JS copy, independent of the freed job.
      send({ identity, expanded: result.buffer }, [result.buffer]);
      return;
    }
    let remaining = maxOutputBytes;
    decodeMs = 0;
    const textures = [];
    for (const file of files) {
      const started = performance.now();
      let decoded;
      try {
        decoded = decode_image(
          new Uint8Array(file.bytes),
          maxDimension,
          remaining,
        );
        const pixels = decoded.take_pixels();
        remaining -= pixels.byteLength;
        textures.push({
          name: file.name,
          width: decoded.width,
          height: decoded.height,
          pixels: pixels.buffer,
        });
      } finally {
        decoded?.free();
        decodeMs += performance.now() - started;
      }
    }
    send(
      { identity, textures },
      textures.map((t) => t.pixels),
    );
  } catch (error) {
    send({
      identity,
      kind: booted ? "decode" : "boot",
      error: String(error),
    });
  }
};
