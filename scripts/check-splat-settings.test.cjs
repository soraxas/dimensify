const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

for (const failure of ["navigation", "page creation"]) {
test(`custom Playwright module preserves the original ${failure} error despite cleanup failures`, () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "splat-settings-failure-"));
  try {
    const module = path.join(temp, "playwright.cjs");
    fs.writeFileSync(module, `module.exports = { chromium: { launch: async () => ({
      newPage: async () => {
        if (${JSON.stringify(failure)} === "page creation") throw new Error("primary page creation failure");
        return { on() {}, goto: async () => { throw new Error("primary navigation failure"); },
        screenshot: async () => { throw new Error("secondary screenshot failure"); },
        evaluate: async () => { throw new Error("secondary observation failure"); } };
      },
      close: async () => { throw new Error("secondary close failure"); }
    }) } };`);
    const result = spawnSync(process.execPath, [path.join(__dirname, "check-splat-settings.cjs"),
      "http://127.0.0.1:1/", path.join(temp, "evidence")], {
      env: { ...process.env, PLAYWRIGHT_MODULE: module }, encoding: "utf8", timeout: 10000,
    });
    assert.equal(result.error, undefined, "checker must close its fixture server and exit");
    assert.equal(result.status, 1);
    assert.ok(result.stderr.includes(`primary ${failure} failure`), result.stderr);
    assert.doesNotMatch(result.stderr, /secondary (screenshot|observation|close) failure/);
  } finally {
    fs.rmSync(temp, { recursive: true, force: true });
  }
});
}

const vm = require("node:vm");

// Exercise the actual acceptance runner with fault injection at external I/O.
// A diagnostics write failure must not hide the original browser failure.
for (const original of [new Error("original navigation failure"), null]) {
  test(`failure diagnostics preserve ${original === null ? "a null rejection" : "the original error"} when writes fail`, async () => {
    let screenshots = 0, browserClosed = false, serverClosed = false;
    const reported = [];
    const page = {
      on() {},
      async goto() { throw original; },
      async screenshot() { screenshots++; throw Error("page closed"); },
      async evaluate() { throw Error("page closed"); },
    };
    const browser = {
      async newPage() { return page; },
      async close() { browserClosed = true; },
    };
    const server = {
      listen(_port, _host, ready) { ready(); },
      address() { return { port: 12345 }; },
      close(done) { serverClosed = true; done(); },
    };
    const dependencies = {
      "playwright": { chromium: { async launch() { return browser; } } },
      "node:fs": { mkdirSync() {}, writeFileSync() { throw Error("disk full"); } },
      "node:http": { createServer() { return server; } },
    };
    const processState = { argv: ["node", "check-splat-settings.cjs", "http://viewer.test/", "/evidence"], env: {}, platform: process.platform };
    const context = {
      require: name => dependencies[name] || require(name),
      __dirname, process: processState, URL, Buffer, setTimeout,
      console: { error: value => reported.push(value), log() {} },
    };
    const source = fs.readFileSync(path.join(__dirname, "check-splat-settings.cjs"), "utf8");
    await vm.runInNewContext(source, context, { filename: "check-splat-settings.cjs" });
    assert.deepEqual(reported, [original]);
    assert.equal(screenshots, 1, "A failed JSON write must still attempt the failure screenshot");
    assert.equal(browserClosed, true);
    assert.equal(serverClosed, true);
    assert.equal(processState.exitCode, 1);
  });
}
