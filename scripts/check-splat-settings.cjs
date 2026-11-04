// Browser acceptance for the shared Feathers settings panel. Real canvas input;
// perf=1 observations provide positions and state, never invoke viewer actions.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const fs = require("node:fs");
const path = require("node:path");
const http = require("node:http");
const assert = require("node:assert/strict");

(async () => {
  const url = process.argv[2];
  if (!url) throw new Error("Usage: node scripts/check-splat-settings.cjs SITE_URL [EVIDENCE_DIR]");
  const evidence = path.resolve(process.argv[3] || "dist/viewer-settings/acceptance");
  fs.mkdirSync(evidence, { recursive: true });
  const fixtures = path.resolve(__dirname, "../crates/splat/tests/fixtures/hosted-stream");
  let rejectChunks = false;
  let delayChunks = false;
  let holdChunks = false;
  const heldChunks = [];
  const releaseChunks = () => {
    holdChunks = false;
    for (const send of heldChunks.splice(0)) send();
  };
  const server = http.createServer((req, res) => {
    res.setHeader("Access-Control-Allow-Origin", "*");
    const relative = new URL(req.url, "http://localhost").pathname.slice(1);
    const file = path.resolve(fixtures, relative);
    if (!file.startsWith(fixtures + path.sep) || !fs.existsSync(file) || (rejectChunks && relative !== "lod-meta.json")) {
      res.writeHead(404); res.end(); return;
    }
    const send = () => res.end(fs.readFileSync(file));
    if (holdChunks && relative !== "lod-meta.json") heldChunks.push(send);
    else if (delayChunks && relative !== "lod-meta.json") setTimeout(send, 1500);
    else send();
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  let browser;
  try {
    browser = await chromium.launch({ headless: true,
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: ["--enable-unsafe-webgpu", ...(process.platform === "darwin" ? ["--use-angle=metal"] : ["--enable-features=Vulkan", "--disable-vulkan-surface"])] });
  } catch (error) {
    if (browser) await browser.close().catch(() => {});
    await new Promise(resolve => server.close(resolve));
    throw error;
  }
  let page;
  try {
    page = await browser.newPage({ viewport: { width: 1000, height: 760 } });
  } catch (error) {
    await browser.close().catch(() => {});
    await new Promise(resolve => server.close(resolve));
    throw error;
  }
  const errors = [], logs = [];
  page.on("pageerror", e => errors.push(String(e)));
  page.on("console", m => logs.push(m.text()));
  const capture = name => page.screenshot({ path: path.join(evidence, `${name}.png`) });
  const observed = () => page.evaluate(() => window.__DIMENSIFY_PERF__);
  const wait = async (predicate, message) => {
    const deadline = Date.now() + 45000;
    while (!predicate(await observed())) {
      if (errors.length) throw new Error(errors.join("\n"));
      if (Date.now() > deadline) throw new Error(message);
      await page.waitForTimeout(100);
    }
  };
  const click = async (field, direction = 1) => {
    // Scroll the real panel until its computed control center is inside it.
    for (let attempt = 0; attempt < 25; attempt++) {
      const state = await observed();
      const control = state.viewer_controls.find(c => c.field === field && c.direction === direction);
      if (!control) throw new Error(`Missing control ${field}/${direction}`);
      const height = page.viewportSize().height;
      if (control.y > 110 && control.y < height - 140) {
        await page.mouse.click(control.x, control.y, { delay: 120 });
        await page.waitForTimeout(300); return;
      }
      await page.mouse.move(page.viewportSize().width - 120, height / 2);
      await page.mouse.wheel(0, control.y < 110 ? -240 : 240);
      await page.waitForTimeout(200);
    }
    throw new Error(`Cannot reach control ${field}`);
  };
  const toolbar = async action => {
    const control = (await observed()).viewer_toolbar.find(c => c.action === action);
    assert.ok(control, `Missing toolbar action ${action}`);
    await page.mouse.click(control.x, control.y, { delay: 120 });
    await page.waitForTimeout(300);
  };
  const togglePanel = () => toolbar("Settings");
  const coordinates = async label => {
    await toolbar("Up");
    await wait(s => s.viewer_coordinates?.some(c => c.label === label && c.y > 90 && c.y < 620), "Coordinate choices did not scroll into view");
    const choice = (await observed()).viewer_coordinates.find(c => c.label === label);
    const camera = (await observed()).camera;
    await page.mouse.click(choice.x, choice.y, { delay: 120 });
    await wait(s => s.source_coordinates === label, "Coordinate preset did not apply");
    assert.deepEqual((await observed()).camera, camera, "Source orientation moved the authored camera");
  };
  try {
    const target = new URL(url); target.searchParams.set("perf", "1"); target.searchParams.set("perf_ui", "1");
    await page.goto(target.href);
    await wait(s => s?.viewer_controls, "Viewer did not publish settings observations");
    await togglePanel();
    await wait(s => s.viewer_settings.open, "Settings did not open");
    await capture("settings-default");
    const initial = (await observed()).settings;
    await click("Transparency");
    await wait(s => s.settings.transparency === "stochastic", "Stochastic toggle failed");
    await click("Transparency");
    await click("Footprint");
    await wait(s => s.settings.target_pixels === initial.target_pixels + 1, "Footprint step failed");
    await click("Footprint", -1);
    await click("Splats", -1);
    await wait(s => s.settings.splat_budget === initial.splat_budget - 250000, "Splat ceiling failed");
    await click("Splats");
    // Edit a real Feathers number field; camera shortcuts stay captured by it.
    await click("Cache", 0);
    const beforeEdit = (await observed()).camera;
    await page.keyboard.press("w", { delay: 120 });
    await page.waitForTimeout(200);
    assert.deepEqual((await observed()).camera, beforeEdit, "Focused numeric input leaked Fly movement");
    // Collapse the automatic initial selection, then exercise platform Select All.
    await page.keyboard.press("ArrowRight", { delay: 120 });
    await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A", { delay: 120 });
    await page.keyboard.type("128", { delay: 60 });
    await page.keyboard.press("Enter", { delay: 120 });
    await wait(s => s.settings.cpu_cache_bytes === 128 * 1024 * 1024, "Numeric cache editor failed");
    assert.deepEqual((await observed()).camera, beforeEdit, "Numeric editing moved the camera");
    await click("Advanced");
    await click("Precision");
    await wait(s => s.viewer_settings.radix_bits === "Bits16", "Radix precision failed");
    await click("Precision"); await click("Precision");
    await click("Sort");
    await wait(s => s.viewer_settings.cpu_sort_requested, "CPU sort toggle failed");
    await click("Sort");
    await capture("settings-advanced");
    await coordinates("3DGS / SuperSplat");
    await capture("coordinates-3dgs");
    await coordinates("+Z");
    await togglePanel();
    holdChunks = true;
    await toolbar("Url");
    await page.waitForTimeout(300);
    await page.keyboard.type(`${origin}/lod-meta.json`, { delay: 30 });
    await page.waitForTimeout(300);
    await page.keyboard.press("Enter");
    await wait(s => s.stream_count === 1 && s.phase === "loading", "Initial stream was not staged");
    await coordinates("3DGS / SuperSplat");
    await click("Backend");
    await wait(s => s.viewer_controls.find(c => c.field === "Apply").disabled, "Apply enabled during initial load");
    await click("Apply", 0);
    assert.equal((await observed()).viewer_settings.active_backend, "packed");
    assert.equal((await observed()).viewer_settings.applying, false);
    await click("Compaction");
    await wait(s => s.viewer_settings.compaction_requested, "Live compaction did not apply during load");
    releaseChunks();
    await wait(s => s.phase === "ready" && s.stream_count === 1 && s.render_ready_splats > 0 && !s.streaming_busy, "Stream did not become ready");
    assert.equal((await observed()).source_coordinates, "3DGS / SuperSplat", "Pending stream lost its source preset");
    await coordinates("-Y");
    await coordinates("+Z");
    assert.equal((await observed()).viewer_settings.compaction_requested, true, "Normal scene commit reverted live compaction");
    assert.equal((await observed()).viewer_settings.active_backend, "packed");
    await click("Compaction");
    await click("Backend"); await click("Backend");
    await togglePanel();
    await toolbar("Frame");
    await page.waitForTimeout(1000);
    const camera = (await observed()).camera;
    await togglePanel();
    await click("Backend");
    await wait(s => s.viewer_settings.pending_backend === "gpu" && s.viewer_settings.active_backend === "packed", "Draft backend changed active scene");
    delayChunks = true;
    await click("Apply", 0);
    await wait(s => s.viewer_settings.applying && s.stream_count === 2 && s.render_ready_splats > 0, "Reload did not retain old scene");
    await capture("reload-pending");
    await wait(s => !s.viewer_settings.applying && s.viewer_settings.active_backend === "gpu" && s.stream_count === 1, "GPU reload failed");
    assert.deepEqual((await observed()).camera, camera, "Reload changed camera pose");
    await capture("reload-gpu");
    delayChunks = false;
    await click("Backend");
    await click("Apply", 0);
    await wait(s => !s.viewer_settings.applying && s.viewer_settings.active_backend === "snapshot" && s.stream_count === 1, "Snapshot reload failed");
    await click("Sort");
    await wait(s => s.viewer_settings.cpu_sort_requested, "Snapshot CPU sorting unavailable");
    await click("Sort");
    await click("Backend");
    await click("Projection");
    rejectChunks = true;
    await click("Apply", 0);
    await wait(s => !s.viewer_settings.applying && s.phase === "failed" && s.viewer_settings.notice.length > 0, "Failed replacement did not report failure");
    assert.equal((await observed()).viewer_settings.active_backend, "snapshot");
    assert.equal((await observed()).stream_count, 1);
    assert.deepEqual((await observed()).camera, camera);
    await capture("reload-failed-retained");
    rejectChunks = false;
    await click("Apply", 0);
    await wait(s => !s.viewer_settings.applying && s.viewer_settings.active_backend === "packed" && s.viewer_settings.projection_cache && s.stream_count === 1, "Projection reload failed");
    await capture("reload-projection");
    // A failed local import must leave that successful stream reloadable.
    await togglePanel();
    const chooser = page.waitForEvent("filechooser");
    await toolbar("Open");
    await (await chooser).setFiles({ name: "broken.sog", mimeType: "application/octet-stream", buffer: Buffer.from("bad archive") });
    await wait(s => s.phase === "failed", "Invalid file did not fail");
    await togglePanel();
    await click("Projection"); await click("Apply", 0);
    await wait(s => !s.viewer_settings.applying && !s.viewer_settings.projection_cache && s.phase === "ready", "Invalid file discarded current stream reload source");
    await click("Reset", 0);
    await wait(s => s.settings.splat_budget === initial.splat_budget && s.settings.target_pixels === initial.target_pixels, "Reset failed");
    // Layout/touch emulation is correctness coverage, not physical phone performance.
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(500);
    await click("Advanced");
    await capture("settings-narrow");
    const panel = (await observed()).viewer_panel;
    assert.ok(panel && panel.width > 0 && panel.height > 0, "Open panel must publish computed bounds");
    const viewport = page.viewportSize();
    const left = Math.max(0, panel.x - panel.width / 2);
    const right = Math.min(viewport.width, panel.x + panel.width / 2);
    const top = Math.max(140, panel.y - panel.height / 2);
    const bottom = Math.min(viewport.height - 140, panel.y + panel.height / 2);
    assert.ok(right - left > 20 && bottom - top > 120, "Panel must expose a usable touch region");
    // Use the panel's background inset, away from controls; both points must
    // be inside its computed rectangle and the visible canvas area.
    const start = { x: right - 6, y: top + (bottom - top) * 0.75, id: 1 };
    const end = { ...start, y: top + (bottom - top) * 0.25 };
    for (const point of [start, end]) {
      assert.ok(Math.abs(point.x - panel.x) < panel.width / 2 && Math.abs(point.y - panel.y) < panel.height / 2,
        "Touch point must land inside the observed settings panel");
    }
    const beforeTouch = (await observed()).camera;
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 1 });
    const drag = async () => {
      await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [start] });
      await page.waitForTimeout(100);
      for (let step = 1; step <= 6; step++) {
        await cdp.send("Input.dispatchTouchEvent", { type: "touchMove",
          touchPoints: [{ ...start, y: start.y + (end.y - start.y) * step / 6 }] });
        await page.waitForTimeout(40);
      }
      await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
    };
    await drag();
    await page.waitForTimeout(500);
    assert.deepEqual((await observed()).camera, beforeTouch, "Panel touch scrolled camera");
    await capture("settings-narrow-scrolled");
    // The narrow panel overlaps the toolbar; use its actual Close control.
    await click("Close", 0);
    await wait(s => !s.viewer_settings.open && s.viewer_panel === null, "Panel did not close for canvas control gesture");
    const beforeCanvasTouch = (await observed()).camera;
    await drag();
    await wait(s => JSON.stringify(s.camera) !== JSON.stringify(beforeCanvasTouch), "Canvas control touch did not move the camera");
    await capture("settings-canvas-touch-control");
    if (errors.length) throw new Error(errors.join("\n"));
    fs.writeFileSync(path.join(evidence, "result.json"), JSON.stringify({ result: "PASS", browser: browser.version(), checks: ["coordinate presets after scrolling and during pending/live streaming", "numeric cache editor and platform Select All", "quality steppers and reset", "precision and applicable CPU sorting", "explicit packed/GPU/snapshot reloads", "old scene visible until replacement ready", "camera preserved", "failed reload rollback and retry", "failed file retains reload source", "projection cache reload", "narrow layout and panel touch ownership", "canvas control touch moves camera"], final: await observed() }, null, 2));
    console.log(`PASS: viewer settings; evidence ${evidence}`);
  } catch (error) {
    // A crashed/closed page may reject both diagnostics; retain the first failure.
    fs.writeFileSync(path.join(evidence, "failure.json"), JSON.stringify({ error: String(error), stack: error.stack }, null, 2));
    await capture("failure").catch(() => {});
    try {
      fs.writeFileSync(path.join(evidence, "failure-state.json"), JSON.stringify((await observed()) ?? null, null, 2));
    } catch {}
    throw error;
  } finally {
    releaseChunks();
    fs.writeFileSync(path.join(evidence, "browser.log"), logs.join("\n"));
    await browser.close().catch(() => {});
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
