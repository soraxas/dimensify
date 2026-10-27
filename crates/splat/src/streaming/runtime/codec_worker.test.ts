import { expect, test } from "bun:test";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import fixture from "./fixtures/worker-expanded-v2.json";

// Exercise the real compiled worker codec and wasm-bindgen Vec transfer, not a
// mock decoder. Build the viewer/codec first; SOG_CODEC_DIR selects another build.
const codecDir = resolve(
  process.env.SOG_CODEC_DIR ?? "dist/sog-codec",
);
const hasCodec = await Bun.file(`${codecDir}/sog_codec_bg.wasm`).exists();
if (process.env.SOG_CODEC_DIR && !hasCodec)
  throw Error(`Missing requested worker build: ${codecDir}`);
const glue = hasCodec
  ? await import(pathToFileURL(`${codecDir}/sog_codec.js`).href)
  : undefined;
if (glue)
  await glue.default({
    module_or_path: await Bun.file(
      `${codecDir}/sog_codec_bg.wasm`,
    ).arrayBuffer(),
  });

for (const coefficientsPerChannel of [1, 16]) {
  test.skipIf(!hasCodec)(
    `worker WASM expanded wire preserves SH${coefficientsPerChannel === 1 ? 0 : 3} values and plane order`,
    () => {
      const job = new glue.ExpandedJob(
        JSON.stringify(fixture.meta),
        coefficientsPerChannel,
      );
      try {
        for (const [name, encoded] of Object.entries(fixture.files)) {
          if (
            coefficientsPerChannel === 1 &&
            ["centroids.webp", "labels.webp"].includes(name)
          )
            continue;
          job.add(name, Buffer.from(encoded, "base64"));
        }
        const wire: Float32Array = job.finish();
        const coefficients = coefficientsPerChannel === 1 ? 4 : 48;
        const expected = new Float32Array(32 * (12 + coefficients));
        for (let row = 0; row < 32; row++) expected[row * 4 + 3] = 1;
        expected[4] = 1;
        expected.set(
          [1, 0, 0, 0, 0.99998844, 0.0027730097, 0.0027730097, 0.0027730097],
          32 * 4,
        );
        expected.set([1, 2, 4, 0, 4, 2, 1, 128 / 255], 32 * 8);
        expected.set([0.25, -0.5, 1.25], 32 * 12);
        expected.set([1.25, -0.5, 0.25], 32 * 12 + coefficients);
        if (coefficientsPerChannel === 16) {
          for (let coefficient = 1; coefficient < 16; coefficient++) {
            expected.set([0.125, -0.25, 0.5], 32 * 12 + coefficient * 3);
          }
        }
        expect(wire.length).toBe(expected.length);
        for (let i = 0; i < expected.length; i++)
          expect(wire[i]).toBeCloseTo(expected[i], 6);
      } finally {
        job.free();
      }
    },
  );
}
