// Engine-independent worker. The same Rust WebP codec preserves arbitrary RGBA
// data, including colour bytes under zero alpha. No Canvas/ImageDecoder path.

let consumed = false;
self.onmessage = async ({ data }) => {
  if (consumed) return; // One bounded job per worker; terminate to release WASM high water.
  consumed = true;
  const { identity, files, maxDimension, maxOutputBytes, expanded } = data;
  let booted = false;
  try {
    // Content version comes from the app's compiled codec/source fingerprint,
    // not a timestamp. Version both glue and WASM to avoid mixed cached builds.
    const glue = new URL("./sog-codec/sog_codec.js", self.location.href);
    const wasm = new URL("./sog-codec/sog_codec_bg.wasm", self.location.href);
    glue.search = wasm.search = self.location.search;
    const {
      default: init,
      decode_image,
      ExpandedJob,
    } = await import(glue.href);
    await init({ module_or_path: wasm });
    booted = true;
    if (expanded) {
      const started = performance.now();
      const job = new ExpandedJob(
        expanded.meta,
        expanded.coefficientsPerChannel,
      );
      try {
        for (const file of files)
          job.add(file.name, new Uint8Array(file.bytes));
        const result = job.finish();
        self.postMessage(
          {
            identity,
            expanded: result.buffer,
            decodeMs: performance.now() - started,
          },
          [result.buffer],
        );
      } finally {
        job.free();
      }
      return;
    }
    let remaining = maxOutputBytes;
    let decodeMs = 0;
    const textures = [];
    for (const file of files) {
      const started = performance.now();
      const decoded = decode_image(
        new Uint8Array(file.bytes),
        maxDimension,
        remaining,
      );
      try {
        const pixels = decoded.take_pixels();
        remaining -= pixels.byteLength;
        textures.push({
          name: file.name,
          width: decoded.width,
          height: decoded.height,
          pixels: pixels.buffer,
        });
      } finally {
        decoded.free();
        decodeMs += performance.now() - started;
      }
    }
    self.postMessage(
      { identity, textures, decodeMs },
      textures.map((t) => t.pixels),
    );
  } catch (error) {
    self.postMessage({
      identity,
      kind: booted ? "decode" : "boot",
      error: String(error),
    });
  }
};
