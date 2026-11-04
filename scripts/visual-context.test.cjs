const { test } = require("node:test");
const assert = require("node:assert/strict");
const os = require("node:os");
const fs = require("node:fs");
const path = require("node:path");
const { collect, cpuFlags, attach } = require("./visual-context.cjs");

test("capture host records CPU and an explicit environment allowlist", () => {
  const host = collect({ ImageVersion: "pinned-image", LP_NUM_THREADS: "2", PIXEL_EAGLE_TOKEN: "never-record-this" });
  assert.equal(host.arch, os.arch());
  assert.deepEqual(host.cpu_models, [...new Set(os.cpus().map(cpu => cpu.model))].sort());
  assert.equal(host.runner_image, "pinned-image");
  assert.equal(host.renderer_environment.LP_NUM_THREADS, "2");
  assert(!JSON.stringify(host).includes("never-record-this"));
  assert.deepEqual(cpuFlags("processor: 0\nflags : avx sse fma\nprocessor: 1\nflags : sse avx fma\n"), ["avx", "fma", "sse"]);
});

test("attaching host diagnostics preserves scene identity and rendered bytes", t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "dimensify-context-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const metadata = { sceneHash: "scene", settingsHash: "settings" };
  fs.writeFileSync(path.join(root, "captures.json"), JSON.stringify({ schema: "dimensify-visual-captures/1", metadata, captures: [] }));
  const bytes = Buffer.from("unchanged-image");
  fs.writeFileSync(path.join(root, "scene.png"), bytes);
  attach(root);
  const actual = JSON.parse(fs.readFileSync(path.join(root, "captures.json")));
  assert.equal(actual.metadata.sceneHash, metadata.sceneHash);
  assert.equal(actual.metadata.settingsHash, metadata.settingsHash);
  assert(actual.metadata.captureHost.cpu_models.length > 0);
  assert.deepEqual(fs.readFileSync(path.join(root, "scene.png")), bytes);
});
