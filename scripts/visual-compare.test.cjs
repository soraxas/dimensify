const assert = require("node:assert/strict");
const { test } = require("node:test");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");
const { PNG } = require("pngjs");

const cli = path.join(__dirname, "visual-compare.cjs");
function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-visual-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const actual = path.join(dir, "actual");
  const baseline = path.join(dir, "baseline");
  fs.mkdirSync(actual);
  fs.mkdirSync(baseline);
  const metadata = {
    lane: "native-test", target: "native", backend: "Vulkan", adapter: "test-adapter",
    driver: "test-driver", browser: "none", viewport: { width: 8, height: 4 },
    sceneHash: "a".repeat(64), settingsHash: "b".repeat(64),
  };
  const png = new PNG({ width: 8, height: 4 });
  for (let y = 0; y < 4; y++) for (let x = 0; x < 8; x++) {
    const i = (y * 8 + x) * 4;
    png.data.set(x < 4 ? [220, 0, 30, 255] : [30, 0, 220, 255], i);
  }
  function writePng(image = png) { fs.writeFileSync(path.join(actual, "scene.png"), PNG.sync.write(image)); }
  writePng();
  fs.copyFileSync(path.join(actual, "scene.png"), path.join(baseline, "scene.png"));
  const captures = { schema: "dimensify-visual-captures/1", metadata,
    captures: [{ name: "scene", file: "scene.png", width: 8, height: 4 }] };
  const manifest = { schema: "dimensify-visual-baseline/1", metadata,
    provenance: { reviewed: true, revision: "c".repeat(40), review: "fixture-review" },
    captures: [{ ...captures.captures[0],
      sha256: crypto.createHash("sha256").update(fs.readFileSync(path.join(baseline, "scene.png"))).digest("hex"),
      tolerance: { channelDelta: 2, maxChangedFraction: 0.01, maxMeanError: 0.5,
        calibration: { samples: 3, description: "Three identical synthetic fixtures; 1-level channel perturbation accepted." } },
      regions: [
        { name: "left-cloud", rect: [0, 0, 4, 4], background: [0, 0, 0], foregroundDelta: 20,
          minCoverage: 0.9, maxCoverage: 1, channels: [{ greater: "r", than: "b", minDelta: 100 }] },
        { name: "right-cloud", rect: [4, 0, 4, 4], background: [0, 0, 0], foregroundDelta: 20,
          minCoverage: 0.9, maxCoverage: 1, channels: [{ greater: "b", than: "r", minDelta: 100 }] },
      ],
    }] };
  function save() {
    fs.writeFileSync(path.join(actual, "captures.json"), JSON.stringify(captures));
    fs.writeFileSync(path.join(baseline, "manifest.json"), JSON.stringify(manifest));
  }
  save();
  let n = 0;
  function run(extra = []) {
    const out = path.join(dir, `result-${n++}`);
    const reference = extra.length ? extra : ["--baseline", baseline];
    const result = spawnSync(process.execPath, [cli, "--actual", actual, ...reference, "--out", out], { encoding: "utf8" });
    assert(fs.existsSync(path.join(out, "results.json")), result.stderr);
    return { ...result, out, report: JSON.parse(fs.readFileSync(path.join(out, "results.json"))) };
  }
  return { dir, actual, baseline, png, captures, manifest, save, writePng, run };
}
function fails(f, pattern) {
  const result = f.run();
  assert.equal(result.status, 1);
  assert.equal(result.report.passed, false);
  assert.match(JSON.stringify(result.report), pattern);
  return result;
}

test("identical and calibrated small variations pass with retained artifacts and metrics", t => {
  const f = fixture(t);
  f.png.data[0]--;
  f.writePng();
  const r = f.run();
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.report.passed, true);
  assert(r.report.cases[0].metrics.meanError > 0);
  assert.equal(r.report.cases[0].metrics.changedFraction, 0);
  for (const file of ["expected.png", "actual.png", "diff.png"])
    assert.equal(PNG.sync.read(fs.readFileSync(path.join(r.out, "scene", file))).width, 8);
});

for (const damage of ["blank", "cloud-drop", "wrong-order"]) {
  test(`${damage} fails semantics independently of pixel tolerance, then recovers`, t => {
    const f = fixture(t);
    // The deliberately permissive pixel threshold proves semantic independence.
    Object.assign(f.manifest.captures[0].tolerance, { channelDelta: 255, maxChangedFraction: 1, maxMeanError: 255 });
    f.save();
    const data = Buffer.from(f.png.data);
    for (let i = 0; i < f.png.data.length; i += 4) {
      if (damage === "blank" || (damage === "cloud-drop" && (i / 4) % 8 >= 4)) f.png.data.fill(0, i, i + 3);
      if (damage === "wrong-order") [f.png.data[i], f.png.data[i + 2]] = [f.png.data[i + 2], f.png.data[i]];
    }
    f.writePng();
    const r = fails(f, /semantic/);
    assert.equal(r.report.cases[0].pixelPassed, true);
    f.png.data.set(data);
    f.writePng();
    assert.equal(f.run().status, 0);
  });
}

test("pixel comparison independently catches changed appearance", t => {
  const f = fixture(t);
  for (let i = 0; i < f.png.data.length; i += 4) f.png.data[i + 1] = 40;
  f.writePng();
  const r = fails(f, /pixel/);
  assert.equal(r.report.cases[0].semanticPassed, true);
});

for (const damage of ["missing", "extra", "duplicate-name", "duplicate-file", "wrong-dimensions", "malformed-png", "corrupt-crc", "identity", "baseline-hash", "unreviewed", "missing-baseline", "missing-tolerance", "missing-calibration", "missing-regions", "unsafe-path", "unknown-schema", "symlink", "nan-threshold", "negative-region", "empty-identity"]) {
  test(`fails closed for ${damage}`, t => {
    const f = fixture(t);
    if (damage === "missing") f.captures.captures = [];
    if (damage === "extra") fs.copyFileSync(path.join(f.actual, "scene.png"), path.join(f.actual, "extra.png"));
    if (damage === "duplicate-name") f.captures.captures.push({ ...f.captures.captures[0], file: "other.png" });
    if (damage === "duplicate-file") f.captures.captures.push({ ...f.captures.captures[0], name: "other" });
    if (damage === "wrong-dimensions") f.captures.captures[0].width = 9;
    if (damage === "identity") f.captures.metadata = { ...f.captures.metadata, driver: "different" };
    if (damage === "baseline-hash") f.manifest.captures[0].sha256 = "0".repeat(64);
    if (damage === "unreviewed") f.manifest.provenance.reviewed = false;
    if (damage === "missing-tolerance") delete f.manifest.captures[0].tolerance;
    if (damage === "missing-calibration") delete f.manifest.captures[0].tolerance.calibration;
    if (damage === "missing-regions") f.manifest.captures[0].regions = [];
    if (damage === "nan-threshold") f.manifest.captures[0].tolerance.maxMeanError = null;
    if (damage === "negative-region") f.manifest.captures[0].regions[0].rect[0] = -1;
    if (damage === "empty-identity") f.captures.metadata = { ...f.captures.metadata, backend: "" };
    if (damage === "unsafe-path") f.captures.captures[0].file = "../scene.png";
    if (damage === "unknown-schema") f.captures.schema = "unknown";
    f.save();
    if (damage === "malformed-png") fs.writeFileSync(path.join(f.actual, "scene.png"), "not a PNG");
    if (damage === "corrupt-crc") {
      const bytes = fs.readFileSync(path.join(f.actual, "scene.png"));
      bytes[29] ^= 1;
      fs.writeFileSync(path.join(f.actual, "scene.png"), bytes);
    }
    if (damage === "symlink") {
      fs.unlinkSync(path.join(f.actual, "scene.png"));
      fs.symlinkSync(path.join(f.baseline, "scene.png"), path.join(f.actual, "scene.png"));
    }
    if (damage === "missing-baseline") fs.unlinkSync(path.join(f.baseline, "manifest.json"));
    const failed = fails(f, /error/i);
    if (damage === "missing-baseline" || damage === "identity") assert.equal(failed.report.goldenCompared, false);
  });
}

test("baseline semantics must themselves match the reviewed PNG", t => {
  const f = fixture(t);
  f.manifest.captures[0].regions[0].channels[0].greater = "b";
  f.manifest.captures[0].regions[0].channels[0].than = "r";
  f.save();
  fails(f, /baseline.*semantic/);
});

test("explicit semantic-only assertions work without goldens and never claim a pixel verdict", t => {
  const f = fixture(t);
  const expectations = path.join(f.dir, "expectations.json");
  fs.writeFileSync(expectations, JSON.stringify({
    schema: "dimensify-visual-expectations/1", viewport: f.captures.metadata.viewport,
    captures: f.manifest.captures.map(({ name, width, height, regions }) => ({ name, width, height, regions })),
  }));
  fs.rmSync(f.baseline, { recursive: true });
  const args = ["--semantic-only", "--expectations", expectations];
  const r = f.run(args);
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.report.mode, "semantic-only");
  assert.equal(r.report.goldenCompared, false);
  assert.equal(r.report.cases[0].pixelPassed, undefined);
  f.png.data.fill(0);
  f.writePng();
  const failed = f.run(args);
  assert.equal(failed.status, 1);
  assert.match(JSON.stringify(failed.report), /semantic/);
});

test("semantic-only forbids simultaneous baseline and expectations arguments", t => {
  const f = fixture(t);
  const r = spawnSync(process.execPath, [cli, "--actual", f.actual, "--out", path.join(f.dir, "out"), "--semantic-only", "--baseline", f.baseline, "--expectations", path.join(f.dir, "expectations.json")]);
  assert.equal(r.status, 1);
});

test("all expected cases are evaluated and retain diffs after a case fails", t => {
  const f = fixture(t);
  for (const dir of [f.actual, f.baseline]) fs.copyFileSync(path.join(dir, "scene.png"), path.join(dir, "second.png"));
  f.captures.captures.push({ ...f.captures.captures[0], name: "second", file: "second.png" });
  f.manifest.captures.push({ ...f.manifest.captures[0], name: "second", file: "second.png" });
  f.save();
  f.png.data.fill(0);
  f.writePng();
  const r = fails(f, /semantic/);
  assert.equal(r.report.cases.length, 2);
  assert.equal(r.report.cases[0].passed, false);
  assert.equal(r.report.cases[1].passed, true);
  for (const name of ["scene", "second"]) assert(fs.existsSync(path.join(r.out, name, "diff.png")));
});

test("output directory must be new and never overwrites inputs", t => {
  const f = fixture(t);
  const before = fs.readFileSync(path.join(f.actual, "scene.png"));
  const result = spawnSync(process.execPath, [cli, "--actual", f.actual, "--baseline", f.baseline, "--out", f.actual]);
  assert.equal(result.status, 1);
  assert.deepEqual(fs.readFileSync(path.join(f.actual, "scene.png")), before);
});


test("reviewed hosted Vulkan noise passes while delta-2 and excess total error fail", t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-vulkan-calibration-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const reference = path.resolve(__dirname, "../tests/visual/baselines/native-vulkan-github");
  const observed = path.resolve(__dirname, "../tests/visual/calibration/native-vulkan-second-run");
  const actual = path.join(root, "actual");
  fs.mkdirSync(actual);
  for (const file of fs.readdirSync(observed)) fs.copyFileSync(path.join(observed, file), path.join(actual, file));
  let attempt = 0;
  const run = () => {
    const out = path.join(root, `result-${attempt++}`);
    const result = spawnSync(process.execPath, [cli, "--actual", actual, "--baseline", reference, "--out", out], { encoding: "utf8" });
    return { status: result.status, report: JSON.parse(fs.readFileSync(path.join(out, "results.json"))) };
  };
  const noise = run();
  assert.equal(noise.status, 0);
  assert.equal(noise.report.cases.length, 5);
  assert(noise.report.cases.every(entry => entry.metrics.maxError === 1));
  // Start boundary faults from the original reference, excluding measured noise.
  for (const file of fs.readdirSync(reference).filter(file => file.endsWith(".png"))) fs.copyFileSync(path.join(reference, file), path.join(actual, file));
  const image = PNG.sync.read(fs.readFileSync(path.join(actual, "anisotropic.png")));
  const original = Buffer.from(image.data);
  for (const [count, delta, accepted] of [[1, 2, false], [524, 1, true], [525, 1, false]]) {
    image.data.set(original);
    for (let i = 0; i < count; i++) image.data[i * 4] += delta;
    fs.writeFileSync(path.join(actual, "anisotropic.png"), PNG.sync.write(image));
    const result = run();
    const changed = result.report.cases.find(entry => entry.name === "anisotropic");
    assert.equal(result.status, accepted ? 0 : 1);
    assert.equal(changed.semanticPassed, true);
    assert.equal(changed.pixelPassed, accepted);
  }
});
