const assert = require("node:assert/strict");
const { test } = require("node:test");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { PNG } = require("pngjs");

const creator = path.join(__dirname, "visual-baseline.cjs");
const comparator = path.join(__dirname, "visual-compare.cjs");
const revision = "c".repeat(40);
function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-baseline-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const actual = path.join(dir, "actual");
  const repeat = path.join(dir, "repeat");
  const out = path.join(dir, "baseline");
  const expectations = path.join(dir, "expectations.json");
  fs.mkdirSync(actual);
  fs.mkdirSync(repeat);
  const png = new PNG({ width: 4, height: 4 });
  for (let i = 0; i < png.data.length; i += 4) png.data.set([220, 20, 30, 255], i);
  const metadata = { lane: "native-test", target: "native", backend: "Vulkan", adapter: "test-adapter", driver: "test-driver", browser: "none",
    viewport: { width: 4, height: 4 }, sceneHash: "a".repeat(64), settingsHash: "b".repeat(64), build: { commit: revision, dirty: "false", profile: "ci" } };
  const manifest = { schema: "dimensify-visual-captures/1", metadata,
    captures: [{ name: "scene", file: "scene.png", width: 4, height: 4 }] };
  const expected = { schema: "dimensify-visual-expectations/1", viewport: metadata.viewport,
    captures: [{ name: "scene", width: 4, height: 4, regions: [{ name: "cloud", rect: [0, 0, 4, 4], background: [0, 0, 0], foregroundDelta: 20, minCoverage: 1, maxCoverage: 1,
      channels: [{ greater: "r", than: "b", minDelta: 100 }] }] }] };
  fs.writeFileSync(expectations, JSON.stringify(expected));
  function save(root = actual, value = manifest, image = png) {
    fs.writeFileSync(path.join(root, "captures.json"), JSON.stringify(value));
    fs.writeFileSync(path.join(root, "scene.png"), PNG.sync.write(image));
  }
  save(); save(repeat);
  function run(options = {}) {
    return spawnSync(process.execPath, [creator, "--actual", actual, "--repeat", options.repeat || repeat, "--expectations", expectations, "--out", out, "--revision", revision, "--review", "Explicit synthetic fixture review"], { encoding: "utf8" });
  }
  return { dir, actual, repeat, out, expectations, png, manifest, expected, save, run };
}

test("explicit reviewed creation records two identical decoded captures and passes strict comparison", t => {
  const f = fixture(t);
  // Differently encoded PNGs still calibrate identical decoded RGBA.
  fs.writeFileSync(path.join(f.repeat, "scene.png"), PNG.sync.write(f.png, { deflateLevel: 0 }));
  const result = f.run();
  assert.equal(result.status, 0, result.stderr);
  const baseline = JSON.parse(fs.readFileSync(path.join(f.out, "manifest.json")));
  assert.deepEqual(baseline.provenance, { reviewed: true, revision, review: "Explicit synthetic fixture review" });
  const limits = baseline.captures[0].tolerance;
  assert.equal(limits.channelDelta, 0);
  assert.equal(limits.maxChangedFraction, 0);
  assert.equal(limits.maxMeanError, 0);
  assert.equal(limits.calibration.samples, 2);
  assert.equal(limits.calibration.runs.length, 2);
  const comparison = spawnSync(process.execPath, [comparator, "--actual", f.actual, "--baseline", f.out, "--out", path.join(f.dir, "comparison")], { encoding: "utf8" });
  assert.equal(comparison.status, 0, comparison.stderr);
  f.png.data[0]--;
  f.save(f.actual);
  const changed = spawnSync(process.execPath, [comparator, "--actual", f.actual, "--baseline", f.out, "--out", path.join(f.dir, "changed")], { encoding: "utf8" });
  assert.equal(changed.status, 1);
  assert.match(changed.stderr, /pixel comparison failed/);
});

test("browser process startup duration is diagnostic and may differ between calibration runs", t => {
  const f = fixture(t);
  const other = structuredClone(f.manifest);
  f.manifest.metadata.browserGpuInfo = { auxAttributes: { initializationTime: 0.03, glRenderer: "same" } };
  other.metadata.browserGpuInfo = { auxAttributes: { initializationTime: 0.04, glRenderer: "same" } };
  f.save(f.actual); f.save(f.repeat, other);
  const result = f.run();
  assert.equal(result.status, 0, result.stderr);
});

for (const damage of ["dirty", "missing-dirty", "repeat-dirty", "revision", "repeat-revision", "identity", "extra-metadata", "nonidentical", "semantics", "missing", "extra-file", "malformed-png", "same-directory", "existing-output"]) {
  test(`rejects ${damage} without producing an accepted baseline`, t => {
    const f = fixture(t);
    const other = structuredClone(f.manifest);
    if (damage === "dirty") f.manifest.metadata.build.dirty = true;
    if (damage === "missing-dirty") delete f.manifest.metadata.build.dirty;
    if (damage === "repeat-dirty") other.metadata.build.dirty = "true";
    if (damage === "revision") f.manifest.metadata.build.commit = "e".repeat(40);
    if (damage === "repeat-revision") other.metadata.build.commit = "e".repeat(40);
    if (damage === "identity") other.metadata.driver = "different";
    if (damage === "extra-metadata") other.metadata.extra = "different";
    if (damage === "missing") other.captures = [];
    f.save(f.actual);
    f.save(f.repeat, other);
    if (damage === "nonidentical") {
      f.png.data[1]++;
      f.save(f.repeat, other);
    }
    if (damage === "semantics") {
      f.png.data.fill(0);
      f.save(f.actual); f.save(f.repeat, other);
    }
    if (damage === "extra-file") fs.copyFileSync(path.join(f.actual, "scene.png"), path.join(f.actual, "extra.png"));
    if (damage === "malformed-png") fs.writeFileSync(path.join(f.actual, "scene.png"), "broken");
    if (damage === "existing-output") fs.mkdirSync(f.out);
    const args = damage === "same-directory" ? { repeat: f.actual } : {};
    const result = f.run(args);
    assert.equal(result.status, 1);
    assert.doesNotMatch(result.stderr, /Cannot find module/);
    assert.match(result.stderr, /dirty|revision|identity|Nonidentical|semantic|inventory|PNG|distinct|new/);
    assert(!fs.existsSync(path.join(f.out, "manifest.json")));
  });
}
