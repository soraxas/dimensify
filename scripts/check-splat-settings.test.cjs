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
