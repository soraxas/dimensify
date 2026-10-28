const assert = require("node:assert/strict");
const { test } = require("node:test");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { PNG } = require("pngjs");

const cli = path.join(__dirname, "visual-compare.cjs");
test("local assertions reject Git-based baseline comparisons", t => {
  const f = fixture(t);
  const result = spawnSync(process.execPath, [cli, "--actual", f.actual, "--baseline", f.actual, "--out", path.join(f.dir, "removed-mode")], { encoding: "utf8" });
  assert.equal(result.status, 1, "Image comparison belongs in Pixel Eagle");
});
function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-visual-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const actual = path.join(dir, "actual");
  fs.mkdirSync(actual);
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
  const captures = { schema: "dimensify-visual-captures/1", metadata,
    captures: [{ name: "scene", file: "scene.png", width: 8, height: 4 }] };
  const manifest = { schema: "dimensify-visual-expectations/1", viewport: metadata.viewport,
    captures: [{ name: "scene", width: 8, height: 4,
      regions: [
        { name: "left-cloud", rect: [0, 0, 4, 4], background: [0, 0, 0], foregroundDelta: 20,
          minCoverage: 0.9, maxCoverage: 1, channels: [{ greater: "r", than: "b", minDelta: 100 }] },
        { name: "right-cloud", rect: [4, 0, 4, 4], background: [0, 0, 0], foregroundDelta: 20,
          minCoverage: 0.9, maxCoverage: 1, channels: [{ greater: "b", than: "r", minDelta: 100 }] },
      ],
    }] };
  function save() {
    fs.writeFileSync(path.join(actual, "captures.json"), JSON.stringify(captures));
    fs.writeFileSync(path.join(dir, "expectations.json"), JSON.stringify(manifest));
  }
  save();
  let n = 0;
  function run(extra = []) {
    const out = path.join(dir, `result-${n++}`);
    const reference = extra.length ? extra : ["--expectations", path.join(dir, "expectations.json")];
    const result = spawnSync(process.execPath, [cli, "--actual", actual, ...reference, "--out", out], { encoding: "utf8" });
    assert(fs.existsSync(path.join(out, "results.json")), result.stderr);
    return { ...result, out, report: JSON.parse(fs.readFileSync(path.join(out, "results.json"))) };
  }
  return { dir, actual, png, captures, manifest, save, writePng, run };
}
function fails(f, pattern) {
  const result = f.run();
  assert.equal(result.status, 1);
  assert.equal(result.report.passed, false);
  assert.match(JSON.stringify(result.report), pattern);
  return result;
}

test("analytic assertions need no reference images and retain region results", t => {
  const f = fixture(t);
  const r = f.run();
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.report.passed, true);
  assert.equal(r.report.mode, "semantic-only");
  assert.equal(r.report.goldenCompared, false);
  assert.equal(r.report.cases[0].pixelPassed, undefined);
  assert.equal(r.report.cases[0].regions.length, 2);
  assert.equal(PNG.sync.read(fs.readFileSync(path.join(r.out, "scene", "actual.png"))).width, 8);
});

for (const damage of ["blank", "cloud-drop", "wrong-order"]) {
  test(`${damage} fails analytic assertions, then recovers`, t => {
    const f = fixture(t);
    const data = Buffer.from(f.png.data);
    for (let i = 0; i < f.png.data.length; i += 4) {
      if (damage === "blank" || (damage === "cloud-drop" && (i / 4) % 8 >= 4)) f.png.data.fill(0, i, i + 3);
      if (damage === "wrong-order") [f.png.data[i], f.png.data[i + 2]] = [f.png.data[i + 2], f.png.data[i]];
    }
    f.writePng();
    fails(f, /semantic/);
    f.png.data.set(data);
    f.writePng();
    assert.equal(f.run().status, 0);
  });
}

for (const damage of ["missing", "extra", "duplicate-name", "duplicate-file", "wrong-dimensions", "malformed-png", "corrupt-crc", "missing-regions", "unsafe-path", "unknown-schema", "symlink", "negative-region", "empty-identity"]) {
  test(`fails closed for ${damage}`, t => {
    const f = fixture(t);
    if (damage === "missing") f.captures.captures = [];
    if (damage === "extra") fs.copyFileSync(path.join(f.actual, "scene.png"), path.join(f.actual, "extra.png"));
    if (damage === "duplicate-name") f.captures.captures.push({ ...f.captures.captures[0], file: "other.png" });
    if (damage === "duplicate-file") f.captures.captures.push({ ...f.captures.captures[0], name: "other" });
    if (damage === "wrong-dimensions") f.captures.captures[0].width = 9;
    if (damage === "missing-regions") f.manifest.captures[0].regions = [];
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
      fs.copyFileSync(path.join(f.actual, "scene.png"), path.join(f.dir, "linked.png"));
      fs.unlinkSync(path.join(f.actual, "scene.png"));
      fs.symlinkSync(path.join(f.dir, "linked.png"), path.join(f.actual, "scene.png"));
    }
    fails(f, /error/i);
  });
}

test("all expected cases are evaluated after a case fails", t => {
  const f = fixture(t);
  fs.copyFileSync(path.join(f.actual, "scene.png"), path.join(f.actual, "second.png"));
  f.captures.captures.push({ ...f.captures.captures[0], name: "second", file: "second.png" });
  f.manifest.captures.push({ ...f.manifest.captures[0], name: "second", file: "second.png" });
  f.save();
  f.png.data.fill(0);
  f.writePng();
  const r = fails(f, /semantic/);
  assert.equal(r.report.cases.length, 2);
  assert.equal(r.report.cases[0].passed, false);
  assert.equal(r.report.cases[1].passed, true);
  for (const name of ["scene", "second"]) assert(fs.existsSync(path.join(r.out, name, "actual.png")));
});

test("output directory must be new and never overwrites inputs", t => {
  const f = fixture(t);
  const before = fs.readFileSync(path.join(f.actual, "scene.png"));
  const result = spawnSync(process.execPath, [cli, "--actual", f.actual, "--expectations", path.join(f.dir, "expectations.json"), "--out", f.actual]);
  assert.equal(result.status, 1);
  assert.deepEqual(fs.readFileSync(path.join(f.actual, "scene.png")), before);
});
