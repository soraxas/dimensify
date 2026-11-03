// Verify the packaged site through HTTP under a GitHub project subpath.
// Rendering acceptance is separate; this check runs without a GPU or visible window.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");

(async () => {
  const root = path.resolve(process.argv[2] || "dist/showcase");
  const prefix = "/dimensify/";
  const types = {
    ".html": "text/html",
    ".mjs": "text/javascript",
    ".js": "text/javascript",
    ".css": "text/css",
    ".wasm": "application/wasm",
  };
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://localhost");
    const file = path.resolve(
      root,
      decodeURIComponent(url.pathname.slice(prefix.length)),
    );
    if (
      !url.pathname.startsWith(prefix) ||
      (!file.startsWith(root + path.sep) && file !== root)
    ) {
      res.writeHead(404).end();
      return;
    }
    const source =
      fs.existsSync(file) && fs.statSync(file).isDirectory()
        ? path.join(file, "index.html")
        : file;
    if (!fs.existsSync(source) || !fs.statSync(source).isFile()) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, {
      "Content-Type": types[path.extname(source)] || "application/octet-stream",
    });
    fs.createReadStream(source).pipe(res);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  let browser;
  try {
    const origin = `http://127.0.0.1:${server.address().port}`;
    browser = await chromium.launch({
      headless: true,
      executablePath: process.env.CHROMIUM_EXECUTABLE,
    });
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("response", (response) => {
      if (response.status() >= 400)
        errors.push(`${response.status()} ${response.url()}`);
    });
    await page.goto(origin + prefix);
    await page.waitForURL("**/gallery/");
    await page.locator("#examples article").first().waitFor();
    assert.equal(
      await page.locator("#examples article").count(),
      await page.locator("#examples a.live").count(),
      "Default view should contain only launchable browser demos",
    );
    await page
      .getByRole("button", { name: "All examples", exact: true })
      .click();
    await page.locator('[data-filter="all"][aria-pressed="true"]').waitFor();
    const count = await page.locator("#examples article").count();
    assert(count > 3, "Expected browser and local examples");
    await page.getByRole("button", { name: "Browser", exact: true }).click();
    await page
      .locator('[data-filter="browser"][aria-pressed="true"]')
      .waitFor();
    const live = await page
      .locator("#examples a.live")
      .evaluateAll((links) => links.map((link) => link.href));
    const expected = await page.evaluate(
      async () =>
        (await import("./catalog.mjs")).examples.filter(
          (example) => example.href,
        ).length,
    );
    assert.equal(
      live.length,
      expected,
      "Every declared browser example must be launchable",
    );
    for (const href of live) {
      assert(
        new URL(href).pathname.startsWith(prefix),
        `Escaped project base path: ${href}`,
      );
      const response = await page.request.get(href);
      assert.equal(response.status(), 200, href);
      const markup = await response.text();
      assert(markup.includes("dimensify-canvas"), `Missing canvas in ${href}`);
      const module = markup.match(/import init from ["']([^"']+)["']/);
      assert(module, `Missing WASM loader in ${href}`);
      const baseTag = markup.match(/<base\s+href=["']([^"']+)["']/);
      const documentBase = baseTag ? new URL(baseTag[1], href).href : href;
      const loader = new URL(module[1], documentBase).href;
      assert(
        new URL(loader).pathname.startsWith(prefix),
        "Loader escaped project base path",
      );
      const js = await page.request.get(loader);
      assert.equal(js.status(), 200, "Missing JavaScript bindings");
      // Import without init: resolve real snippet dependencies without a GPU.
      await page.evaluate((url) => import(url).then(() => true), loader);
    }
    if (process.env.GALLERY_SCREENSHOT)
      await page.screenshot({
        path: process.env.GALLERY_SCREENSHOT,
        fullPage: true,
      });
    await page.setViewportSize({ width: 390, height: 844 });
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
      "Gallery overflows mobile width",
    );
    await page.getByRole("button", { name: "Headless API / server" }).click();
    await page
      .locator('[data-filter="headless"][aria-pressed="true"]')
      .waitFor();
    assert(
      (await page.locator("#examples article").count()) > 0,
      "Missing headless examples",
    );
    assert.equal(
      await page.locator("#examples a.live").count(),
      0,
      "Headless examples must not have browser launch links",
    );
    await page
      .getByRole("button", { name: "Gaussian scenes", exact: true })
      .click();
    await page.locator('[data-filter="scenes"][aria-pressed="true"]').waitFor();
    assert(
      (await page.locator("a.scene").count()) >= 3,
      "Missing scene presets",
    );
    for (const link of await page
      .locator("a.scene")
      .evaluateAll((links) => links.map((link) => link.href))) {
      const scene = new URL(link);
      assert.equal(scene.pathname, prefix + "splat-viewer/");
      const asset = new URL(scene.searchParams.get("url"));
      assert(asset.pathname.startsWith(prefix + "scenes/"));
      assert.equal((await page.request.get(asset.href)).status(), 200);
    }
    await page
      .locator('[name="url"]')
      .fill("https://assets.test/example.sog?key=a+b&x=1");
    await page.locator('[name="eye"]').fill("4,3,6");
    await page
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    assert(
      await page.locator("#launch-error").textContent(),
      "Incomplete camera should show an error",
    );
    await page.locator('[name="target"]').fill("0,0,0");
    await page.locator('[name="frame"]').selectOption("0");
    await page.locator('[name="embed"]').check();
    await page
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    const configured = new URL(await page.locator("#launch-url").inputValue());
    assert.equal(
      configured.searchParams.get("url"),
      "https://assets.test/example.sog?key=a+b&x=1",
    );
    assert.equal(configured.searchParams.get("embed"), "1");
    assert.equal(configured.searchParams.get("frame"), "0");
    assert(
      (await page.locator("#launch-embed").inputValue()).includes(
        'allow="fullscreen"',
      ),
    );
    assert(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
      "Scene launcher overflows mobile width",
    );
    await page.reload();
    await page.locator("a.scene").first().waitFor();
    const unsupported = await browser.newPage();
    const wasmRequests = [];
    unsupported.on("request", (request) => {
      if (new URL(request.url()).pathname.endsWith(".wasm"))
        wasmRequests.push(request.url());
    });
    await unsupported.addInitScript(() =>
      Object.defineProperty(navigator, "gpu", { value: undefined }),
    );
    await unsupported.goto(origin + prefix + "splat-viewer/");
    await unsupported.getByRole("alert").waitFor();
    assert(
      (await unsupported.getByRole("alert").textContent()).includes(
        "WebGPU-capable",
      ),
    );
    assert.deepEqual(
      wasmRequests,
      [],
      "Unsupported browsers should fail clearly before downloading WASM",
    );
    await unsupported.close();
    assert.deepEqual(errors, []);
    console.log(
      JSON.stringify({
        browser: browser.version(),
        examples: count,
        browserExamples: live.length,
        projectSubpath: prefix,
        result: "PASS",
      }),
    );
  } finally {
    if (browser) await browser.close();
    await new Promise((resolve) => server.close(resolve));
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
