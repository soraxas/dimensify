// Real WebGPU point-cloud controls. Screenshots stay in ignored artifacts.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { PNG } = require("pngjs");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const { softwareFlags } = require("./visual-browser.cjs");

function difference(a, b) {
  a = PNG.sync.read(a); b = PNG.sync.read(b);
  assert.equal(a.data.length, b.data.length);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (Math.max(...[0, 1, 2].map(c => Math.abs(a.data[i + c] - b.data[i + c]))) > 8) changed++;
  }
  return changed / (a.width * a.height);
}

(async () => {
  const [site, out = "dist/pointcloud-browser-check"] = process.argv.slice(2);
  assert(site, "Usage: node scripts/check-pointcloud-browser.cjs SITE_URL [EVIDENCE]");
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch({
    headless: process.env.POINTCLOUD_BROWSER_HEADED !== "1",
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: softwareFlags(),
  });
  const errors = [], logs = [], checks = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1000, height: 760 }, deviceScaleFactor: 1 });
    page.on("pageerror", e => errors.push(String(e)));
    page.on("console", m => { logs.push(m.text()); if (m.type() === "error") errors.push(m.text()); });
    const capture = (name, clip) => page.screenshot({ path: path.join(out, `${name}.png`), ...(clip ? { clip } : {}) });
    const click = async (x, y) => {
      await page.mouse.move(x, y);
      await page.waitForTimeout(200);
      await page.mouse.down();
      await page.waitForTimeout(200);
      await page.mouse.up();
      await page.waitForTimeout(400);
      assert.deepEqual(errors, []);
    };
    await page.goto(new URL("examples/dimensify/pointcloud_viewer/", site.endsWith("/") ? site : site + "/").href);
    const deadline = Date.now() + 90000;
    while (!logs.some(line => line.includes("PointCloud added"))) {
      assert.deepEqual(errors, []);
      assert(Date.now() < deadline, "Point-cloud renderer did not start");
      await page.waitForTimeout(100);
    }
    await page.waitForTimeout(2000);
    const closed = await capture("desktop-closed");
    await click(55, 726);
    const opened = await capture("desktop-settings");
    assert(difference(closed, opened) > 0.02, "Settings must open a visible panel");
    checks.push("open-settings");
    const cloudClip = { x: 180, y: 180, width: 480, height: 400 };
    const circle = await capture("circular-billboard", cloudClip);
    await click(820, 122);
    await capture("circle-mesh-settings");
    await capture("circle-mesh", cloudClip);
    await click(820, 122);
    const square = await capture("square-billboard", cloudClip);
    assert(difference(circle, square) > 0.0001, "Point shape must change rendered cloud pixels");
    checks.push("circle-mesh-and-square-render");
    await click(820, 174);
    const aligned = await capture("normal-aligned", cloudClip);
    assert(difference(square, aligned) > 0.0001, "Normal alignment must change rendered cloud pixels");
    checks.push("normal-aligned-render");
    await click(820, 232);
    const manual = await capture("manual-size", cloudClip);
    // Each browser-native key event goes through the actual number editor.
    await click(820, 307);
    await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A");
    await page.waitForTimeout(200);
    await page.keyboard.type("3");
    await page.waitForTimeout(200);
    await page.waitForTimeout(500);
    await capture("number-settings");
    const small = await capture("small-points", cloudClip);
    assert(difference(manual, small) > 0.001, "Point-size editor must update the renderer");
    await click(820, 388);
    const blurred = await capture("size-after-blur", cloudClip);
    assert(difference(small, blurred) < 0.001, "Changing numeric focus must preserve the value without Enter");
    checks.push("number-editor-render");
    await page.mouse.move(820, 550);
    await page.mouse.wheel(0, 1200);
    await page.waitForTimeout(700);
    await capture("desktop-settings-bottom");
    await click(820, 596);
    const reset = await capture("reset-defaults", cloudClip);
    assert(difference(circle, reset) < 0.001, "Reset settings must restore the initial rendered cloud");
    checks.push("reset-settings-render");
    await page.setViewportSize({ width: 360, height: 640 });
    await page.mouse.move(200, 200);
    await page.mouse.wheel(0, -1500);
    await page.waitForTimeout(700);
    await capture("narrow-settings");
    await click(65, 606);
    await capture("narrow-closed");
    checks.push("narrow-panel-and-close");
    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(out, "result.json"), JSON.stringify({ checks, errors }, null, 2));
    console.log(JSON.stringify({ result: "PASS", checks, evidence: out }));
  } finally {
    fs.writeFileSync(path.join(out, "browser.log"), logs.join("\n") + "\n" + errors.join("\n"));
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
