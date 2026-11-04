const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { PNG } = require("pngjs");
const { pixelStats, compare } = require("./visual-repeat.cjs");

test("repeat diagnostics distinguish identical bytes, one-level variation and large changes", () => {
  const source = Buffer.from([10, 20, 30, 255, 40, 50, 60, 255]);
  assert.equal(pixelStats(source, source).changedPixels, 0);
  const oneLevel = Buffer.from(source); oneLevel[0]++;
  assert.deepEqual(pixelStats(source, oneLevel), { pixels: 2, changedPixels: 1, changedChannels: 1, maxChannelDelta: 1, meanAbsoluteChannelDelta: 0.125 });
  const large = Buffer.from(source); large[4] = 255;
  assert.equal(pixelStats(source, large).maxChannelDelta, 215);
  assert.throws(() => pixelStats(source, Buffer.alloc(4)), /dimensions/);
});

test("repeat capture comparison validates identity and decodes the actual image bytes", t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-repeat-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const first = path.join(root, "first"), second = path.join(root, "second");
  fs.mkdirSync(first); fs.mkdirSync(second);
  const metadata = { lane: "native-test", target: "native", backend: "Metal", adapter: "test",
    driver: "test", browser: "none", viewport: { width: 256, height: 256 },
    sceneHash: "a".repeat(64), settingsHash: "b".repeat(64) };
  const captures = ["anisotropic", "overlap-front", "overlap-reverse", "sh-front", "sh-reverse"]
    .map(name => ({ name, file: name + ".png", width: 256, height: 256 }));
  const manifest = { schema: "dimensify-visual-captures/1", metadata, captures };
  const image = new PNG({ width: 256, height: 256 });
  const bytes = PNG.sync.write(image);
  for (const directory of [first, second]) {
    fs.writeFileSync(path.join(directory, "captures.json"), JSON.stringify(manifest));
    for (const entry of captures) fs.writeFileSync(path.join(directory, entry.file), bytes);
  }
  assert.equal(compare(first, second).identical, true);
  image.data[0] = 1;
  fs.writeFileSync(path.join(second, captures[0].file), PNG.sync.write(image));
  const result = compare(first, second);
  assert.equal(result.identical, false);
  assert.equal(result.goldenCompared, false);
  assert.equal(result.cases[0].changedPixels, 1);
  assert.equal(result.cases[0].maxChannelDelta, 1);
  manifest.metadata.settingsHash = "c".repeat(64);
  fs.writeFileSync(path.join(second, "captures.json"), JSON.stringify(manifest));
  assert.throws(() => compare(first, second), /identity mismatch: settingsHash/);
});
