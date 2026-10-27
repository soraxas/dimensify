// Headless acceptance for the shared canvas-rendered Feathers viewer.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const fs = require("node:fs");
const path = require("node:path");
const http = require("node:http");

(async () => {
  const url = process.argv[2];
  if (!url)
    throw new Error(
      "Usage: node scripts/check-splat-browser.cjs http://localhost:8080 [evidence-directory]",
    );
  const evidence = path.resolve(
    process.argv[3] || "/tmp/dimensify-splat-viewer",
  );
  fs.mkdirSync(evidence, { recursive: true });
  const fixtures = path.resolve(__dirname, "../crates/splat/tests/fixtures");
  const requestedPaths = [];
  const resourceServer = http.createServer((request, response) => {
    requestedPaths.push(request.url);
    const resourcePath = new URL(request.url, "http://localhost").pathname;
    if (resourcePath.startsWith("/hosted/") || resourcePath.startsWith("/missing/") || resourcePath === "/broken.json") {
      response.setHeader("Access-Control-Allow-Origin", "*");
      const relative = resourcePath.slice("/hosted/".length);
      if (resourcePath.startsWith("/missing/")) {
        if (resourcePath.endsWith("/lod-meta.json")) response.end(fs.readFileSync(path.join(fixtures, "hosted-stream/lod-meta.json")));
        else { response.writeHead(404); response.end(); }
        return;
      }
      if (resourcePath === "/broken.json") { response.end("{}"); return; }
      const resource = path.join(fixtures, "hosted-stream", relative);
      if (!fs.existsSync(resource)) { response.writeHead(404); response.end(); return; }
      const bytes = fs.readFileSync(resource);
      response.writeHead(200, { "Content-Length": bytes.length });
      response.end(bytes); return;
    }
    const filename = new URL(request.url, "http://localhost").pathname;
    response.setHeader("Access-Control-Allow-Origin", "*");
    if (filename === "/redirect.ply") {
      response.writeHead(302, { Location: "/remote.ply" }); response.end(); return;
    }
    const file = filename.endsWith(".sog") ? "tiny.sog" : "tiny.ply";
    const bytes = fs.readFileSync(path.join(fixtures, file));
    response.writeHead(200, { "Content-Length": bytes.length, "Content-Type": "application/octet-stream" });
    response.end(bytes);
  });
  await new Promise((resolve) => resourceServer.listen(0, "127.0.0.1", resolve));
  const resourceOrigin = `http://127.0.0.1:${resourceServer.address().port}`;
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: [
      "--enable-unsafe-webgpu",
      ...(process.platform === "darwin"
        ? ["--use-angle=metal"]
        : ["--enable-features=Vulkan", "--disable-vulkan-surface"]),
    ],
  });
  let page;
  const errors = [];
  const messages = [];
  try {
    page = await browser.newPage({ viewport: { width: 1000, height: 760 } });
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => {
      messages.push(message.text());
      // This request intentionally exercises exhausted initial chunk retries.
      const expectedMissingChunk = message.location().url === `${resourceOrigin}/missing/chunks/meta.json`
        && message.text().includes("404");
      if (!expectedMissingChunk && (message.type() === "error" || message.text().includes("%cERROR%c"))) {
        errors.push(message.text());
        console.error(message.text());
      }
    });
    const waitStatus = async (text, timeout = 180000) => {
      const deadline = Date.now() + timeout;
      while (!messages.some((message) => message.includes(text))) {
        if (errors.length) throw new Error(errors.join("\n"));
        if (Date.now() > deadline)
          throw new Error(`Timed out waiting for viewer status: ${text}`);
        await page.waitForTimeout(100);
      }
    };
    // Positions refer to the retained toolbar in this fixed-size reproduction.
    // These are real canvas clicks, not calls into WASM action exports.
    const choose = async (file) => {
      const promise = page.waitForEvent("filechooser", { timeout: 10000 });
      await page.mouse.click(330, 46, { delay: 120 });
      await (await promise).setFiles(file);
    };
    await page.goto(new URL("gallery/", url.endsWith("/") ? url : `${url}/`).href);
    await page.locator("article").first().waitFor();
    const allCards = await page.locator("article").count();
    await page.getByRole("button", { name: "Native viewer", exact: true }).click();
    if (!await page.locator("article").count() || await page.locator("article").count() !== await page.locator("article .badge.native").count()) throw new Error("Gallery native filter failed");
    await page.getByRole("button", { name: "All examples", exact: true }).click();
    if (await page.locator("article").count() !== allCards) throw new Error("Gallery reset filter failed");
    await page.setViewportSize({ width: 390, height: 844 });
    if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) throw new Error("Gallery overflows mobile viewport");
    await page.screenshot({ path: path.join(evidence, "gallery-mobile.png"), fullPage: true });
    await page.setViewportSize({ width: 1000, height: 760 });
    await page.getByRole("button", { name: "Browser", exact: true }).click();
    await page.locator("article").filter({ has: page.getByRole("heading", { name: "Gaussian splat viewer", exact: true }) }).getByRole("link", { name: "Open browser example", exact: true }).click();
    await page
      .locator("#dimensify-file-input")
      .waitFor({ state: "attached", timeout: 120000 });
    await waitStatus("Open a Gaussian PLY or bundled SOG file.");
    await page.waitForTimeout(1500);
    if (await page.locator("button, select").count())
      throw new Error("The host page contains duplicate viewer controls");
    await page.screenshot({ path: path.join(evidence, "empty.png") });

    await choose(path.join(fixtures, "tiny.ply"));
    await waitStatus("tiny.ply  |  8 Gaussians");
    await page.waitForTimeout(1500);
    await page.screenshot({ path: path.join(evidence, "ply-origin.png") });
    await page.mouse.click(450, 46, { delay: 120 });
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(evidence, "ply.png") });

    const bytes = [...fs.readFileSync(path.join(fixtures, "tiny.sog"))];
    await page.evaluate((data) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(data)], "tiny.sog"));
      document.dispatchEvent(
        new DragEvent("dragenter", { bubbles: true, dataTransfer: transfer }),
      );
      document.dispatchEvent(
        new DragEvent("drop", {
          bubbles: true,
          cancelable: true,
          dataTransfer: transfer,
        }),
      );
    }, bytes);
    await waitStatus("tiny.sog  |  8 Gaussians");
    await page.waitForTimeout(1500);
    await page.mouse.click(450, 46, { delay: 120 });
    await page.waitForTimeout(400);
    await page.screenshot({ path: path.join(evidence, "sog.png") });

    // Compare only the viewport so toolbar hover styling cannot satisfy a
    // movement assertion. All gestures are real browser input on the canvas.
    // Keep the crop below the wrapped toolbar's 44-pixel touch targets.
    const viewport = { x: 100, y: 150, width: 800, height: 470 };
    const captureView = async (name) => page.screenshot({
      path: path.join(evidence, `${name}.png`), clip: viewport,
    });
    const beforeAxis = await captureView("axis-z");
    await page.mouse.click(850, 46, { delay: 120 });
    await page.waitForTimeout(500);
    const axisY = await captureView("axis-y");
    if (beforeAxis.equals(axisY)) throw new Error("File up axis did not rotate the loaded cloud");
    for (const axis of ["+X", "-X", "-Y", "-Z", "+Z"]) {
      await page.mouse.click(850, 46, { delay: 120 });
      await page.waitForTimeout(250);
      await captureView(`axis-${axis.startsWith("+") ? `positive-${axis.slice(1)}` : `negative-${axis.slice(1)}`}`);
    }
    await page.waitForTimeout(500);
    const axisZ = await captureView("axis-z-restored");
    if (!beforeAxis.equals(axisZ)) throw new Error("File up axis did not restore the original cloud");
    await page.mouse.click(574, 46, { delay: 120 });
    await page.mouse.move(600, 380);
    await page.waitForTimeout(500);
    const home = await captureView("camera-home");
    await page.mouse.click(450, 46, { delay: 120 });
    await page.mouse.move(600, 380);
    await page.waitForTimeout(400);
    const flyStart = await captureView("camera-fly-start");
    await page.keyboard.down("w");
    await page.waitForTimeout(600);
    await page.keyboard.up("w");
    await page.waitForTimeout(300);
    const flying = await captureView("camera-fly");
    if (flyStart.equals(flying)) throw new Error("Fly W did not move the viewport");
    await page.mouse.click(574, 46, { delay: 120 });
    await page.mouse.move(600, 380);
    await page.waitForTimeout(700);
    const reset = await captureView("camera-reset");
    if (!home.equals(reset)) throw new Error("Reset did not restore the origin or clear momentum");
    await page.mouse.click(698, 46, { delay: 120 });
    await page.mouse.click(450, 46, { delay: 120 });
    await page.waitForTimeout(500);
    const initial = await captureView("camera-initial");
    await page.mouse.move(100, 46);
    await page.mouse.down();
    await page.waitForTimeout(120);
    await page.mouse.move(600, 380, { steps: 12 });
    await page.waitForTimeout(120);
    await page.mouse.up();
    await page.waitForTimeout(300);
    const uiDrag = await captureView("camera-ui-drag");
    if (!initial.equals(uiDrag)) throw new Error("A UI-owned press moved the camera");

    const drag = async (button, end) => {
      await page.mouse.move(600, 380);
      await page.mouse.down({ button });
      await page.waitForTimeout(100);
      await page.mouse.move(...end, { steps: 12 });
      await page.mouse.up({ button });
      await page.waitForTimeout(300);
    };
    await drag("left", [680, 420]);
    const orbited = await captureView("camera-orbit");
    if (uiDrag.equals(orbited)) throw new Error("Orbit did not change the viewport");
    await drag("right", [650, 410]);
    const panned = await captureView("camera-pan");
    if (orbited.equals(panned)) throw new Error("Pan did not change the viewport");
    await page.mouse.wheel(0, -150);
    await page.waitForTimeout(400);
    const zoomed = await captureView("camera-zoom");
    if (panned.equals(zoomed)) throw new Error("Zoom did not change the viewport");
    await page.mouse.click(450, 46, { delay: 120 });
    await page.waitForTimeout(400);
    const framed = await captureView("camera-frame");
    if (zoomed.equals(framed)) throw new Error("Frame did not restore scene framing");

    const touchSession = await page.context().newCDPSession(page);
    await touchSession.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 5 });
    const touchPoints = (points) => points.map(([id, x, y]) => ({ id, x, y, radiusX: 4, radiusY: 4, force: 1 }));
    const touchGesture = async (start, end) => {
      await touchSession.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: touchPoints(start) });
      await page.waitForTimeout(80);
      for (let step = 1; step <= 8; step++) {
        const points = start.map(([id, x, y], index) => [id, x + (end[index][1] - x) * step / 8, y + (end[index][2] - y) * step / 8]);
        await touchSession.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: touchPoints(points) });
        await page.waitForTimeout(35);
      }
      await touchSession.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await page.waitForTimeout(450);
    };
    const touchStart = await captureView("touch-start");
    await touchGesture([[1, 400, 350]], [[1, 520, 390]]);
    const touchOrbit = await captureView("touch-orbit");
    if (touchStart.equals(touchOrbit)) throw new Error("One-finger touch did not orbit");
    await touchGesture([[1, 400, 350], [2, 600, 350]], [[1, 350, 390], [2, 680, 390]]);
    const touchPanPinch = await captureView("touch-pan-pinch");
    if (touchOrbit.equals(touchPanPinch)) throw new Error("Two-finger pan/pinch did not change viewport");
    await touchGesture([[1, 100, 46]], [[1, 150, 350]]);
    const touchUi = await captureView("touch-ui");
    if (!touchPanPinch.equals(touchUi)) throw new Error("Touch originating on toolbar took camera ownership");
    await touchSession.send("Emulation.setTouchEmulationEnabled", { enabled: false });
    await touchSession.detach();
    await page.mouse.click(450, 46, { delay: 120 });
    await page.waitForTimeout(400);

    const openUrl = async (resource, submit = true, paste = false) => {
      await page.mouse.click(82, 90, { delay: 120 });
      await page.waitForTimeout(300);
      await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A", { delay: 120 });
      if (paste) {
        await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
        await page.evaluate((value) => navigator.clipboard.writeText(value), resource);
        await page.keyboard.press(process.platform === "darwin" ? "Meta+V" : "Control+V", { delay: 120 });
        await page.waitForTimeout(500);
      } else {
        await page.keyboard.type(resource, { delay: 3 });
      }
      await page.waitForTimeout(300);
      await page.screenshot({ path: path.join(evidence, "url-panel.png") });
      await page.keyboard.press(submit ? "Enter" : "Escape");
    };
    // Ignore a one-level channel rounding difference in an otherwise identical
    // GPU frame; exact PNG equality confuses this with camera motion.
    const sameView = async (a, b) => {
      if (a.equals(b)) return true;
      return page.evaluate(async ([first, second]) => {
        const pixels = async (bytes) => {
          const image = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: "image/png" }));
          const canvas = new OffscreenCanvas(image.width, image.height);
          const context = canvas.getContext("2d");
          context.drawImage(image, 0, 0); image.close();
          return context.getImageData(0, 0, canvas.width, canvas.height).data;
        };
        const [x, y] = await Promise.all([pixels(first), pixels(second)]);
        return x.length === y.length && x.every((value, index) => Math.abs(value - y[index]) <= 1);
      }, [[...a], [...b]]);
    };
    // Wait for the preceding camera gesture to settle before testing input
    // suppression. Browser frame delivery can lag behind the fixed input delay.
    const settledView = async (name) => {
      let previous = await captureView(name);
      let stable = 0;
      const deadline = Date.now() + 10000;
      while (stable < 3) {
        await page.waitForTimeout(200);
        const current = await captureView(name);
        stable = await sameView(previous, current) ? stable + 1 : 0;
        previous = current;
        if (Date.now() > deadline) throw new Error(`Viewport did not settle: ${name}`);
      }
      return previous;
    };
    const beforeEditing = await settledView("url-before-editing");
    await openUrl(`${resourceOrigin}/remote.ply?token=wasdf-test`, false);
    await page.waitForTimeout(400);
    const afterEditing = await captureView("url-after-cancel");
    if (!await sameView(beforeEditing, afterEditing)) throw new Error("Typing URL moved the camera or Escape did not cancel");
    await openUrl(`${resourceOrigin}/redirect.ply?token=signed-test`);
    await waitStatus("redirect.ply  |  8 Gaussians", 30000);
    await page.waitForTimeout(500);
    await page.screenshot({ path: path.join(evidence, "url-ply.png") });
    await openUrl(`${resourceOrigin}/remote.sog?token=signed-test`, true, true);
    await waitStatus("remote.sog  |  8 Gaussians", 30000);
    await page.waitForTimeout(500);
    await page.screenshot({ path: path.join(evidence, "url-sog.png") });
    await openUrl(`${resourceOrigin}/hosted/lod-meta.json?token=manifest-only`);
    await waitStatus("lod-meta.json  |  8 Gaussians", 30000);
    await page.mouse.click(450, 46, { delay: 120 });
    await page.waitForTimeout(1000);
    const streamed = await settledView("manifest-stream");
    if (!requestedPaths.some((value) => value === "/hosted/chunks/meta.json"))
      throw new Error("Manifest did not request chunk metadata");
    if (!requestedPaths.some((value) => value.endsWith("means_l.webp")))
      throw new Error("Manifest did not request constituent textures");
    if (requestedPaths.some((value) => value.includes("chunks/") && value.includes("token=")))
      throw new Error("Manifest credentials leaked to a child URL");
    if (!await page.evaluate(() => window.__DIMENSIFY_SOG_CODEC__?.completed > 0))
      throw new Error("Hosted stream did not complete a real codec worker job");
    await page.mouse.click(850, 46, { delay: 120 });
    await page.waitForTimeout(500);
    const streamedAxis = await captureView("manifest-axis-y");
    if (await sameView(streamed, streamedAxis)) throw new Error("File up axis did not reorient the hosted stream");
    for (let step = 0; step < 5; step++) {
      await page.mouse.click(850, 46, { delay: 120 });
      await page.waitForTimeout(150);
    }
    await page.waitForTimeout(500);
    if (!await sameView(streamed, await captureView("manifest-axis-restored")))
      throw new Error("Hosted stream axis did not restore the displayed scene");
    await openUrl(`${resourceOrigin}/missing/lod-meta.json`);
    await waitStatus("Could not load lod-meta.json", 30000);
    await page.waitForTimeout(500);
    if (!await sameView(streamed, await captureView("manifest-chunk-failure")))
      throw new Error("Exhausted chunk retries replaced the displayed scene");
    await openUrl(`${resourceOrigin}/broken.json`);
    await waitStatus("Could not load broken.json");
    await page.waitForTimeout(500);
    const failedStream = await captureView("manifest-failed-replacement");
    if (!await sameView(streamed, failedStream))
      throw new Error("Failed manifest replacement changed the displayed scene");
    const largeFile = process.env.DIMENSIFY_SPLAT_FILE;
    if (largeFile) {
      await choose(largeFile);
      await waitStatus(`${path.basename(largeFile)}  | `);
      await page.waitForTimeout(3000);
      await page.screenshot({ path: path.join(evidence, "local-scene.png") });
    }
    await choose({
      name: "broken.sog",
      mimeType: "application/octet-stream",
      buffer: Buffer.from("not a zip"),
    });
    await waitStatus("Could not load broken.sog");
    await page.waitForTimeout(500);
    await page.screenshot({ path: path.join(evidence, "invalid.png") });
    if (errors.length) throw new Error(errors.join("\n"));
    console.log(
      JSON.stringify({
        result: "PASS",
        browser: browser.version(),
        evidence,
        checks: [
          "example gallery filters and mobile layout",
          "shared Feathers controls",
          "canvas Open File invokes system chooser",
          "PLY load",
          "SOG drop",
          "canvas Frame",
          "live file up axis changes and restores cloud",
          "Fly movement and Reset clears momentum",
          "Fly/Orbit mode switch",
          "UI drag ownership",
          "orbit/pan/zoom change viewport",
          "real browser touch orbit, pan/pinch and toolbar ownership",
          "invalid replacement",
          "Open URL retained field and Enter submission",
          "cross-origin PLY redirect and SOG with signed query",
          "hosted manifest fetches metadata and textures through shared streaming",
          "manifest credentials are not forwarded to child resources",
          "invalid manifest and exhausted chunk retries retain displayed scene",
          "hosted stream uses the real worker and live source-axis controls",
        ],
      }),
    );
  } catch (error) {
    console.error("Recent viewer logs:", messages.slice(-12));
    if (page)
      await page.screenshot({ path: path.join(evidence, "failure.png") });
    throw error;
  } finally {
    fs.writeFileSync(path.join(evidence, "browser.log"), messages.join("\n"));
    await browser.close();
    await new Promise((resolve) => resourceServer.close(resolve));
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
