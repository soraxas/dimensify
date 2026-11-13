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
    const count = await page.locator("#examples article").count();
    const total = await page.evaluate(
      async () => (await import("./catalog.mjs")).examples.length,
    );
    assert.equal(count, total, "Default view must expose the complete catalog");
    assert.equal(
      await page.locator("[data-filter]").first().getAttribute("data-filter"),
      "all",
      "All examples must be the leftmost filter",
    );
    assert.equal(
      await page.locator('[data-filter="all"]').getAttribute("aria-pressed"),
      "true",
    );
    assert.equal(
      await page.locator("#launch-form").count(),
      0,
      "The link builder belongs on a separate page",
    );
    const splat = page.locator("article").filter({
      has: page.locator('a.live[href="../splat-viewer/"]'),
    });
    assert.equal(
      await splat
        .getByRole("link", { name: "Open scene", exact: true })
        .count(),
      1,
    );
    const builder = splat.getByRole("link", {
      name: "Build a link",
      exact: true,
    });
    const builderURL = new URL(await builder.getAttribute("href"), page.url())
      .href;
    assert.equal(
      new URL(builderURL).pathname,
      prefix + "gallery/build-link.html",
    );
    const pointcloud = page.locator("article").filter({
      has: page.locator('a.live[href="../examples/dimensify/pointcloud_viewer/"]'),
    });
    const pointcloudBuilderURL = new URL(
      await pointcloud.getByRole("link", { name: "Build a link", exact: true }).getAttribute("href"),
      page.url(),
    ).href;
    assert.equal(new URL(pointcloudBuilderURL).searchParams.get("viewer"), "pointcloud");
    if (process.env.GALLERY_SCREENSHOT)
      await page.screenshot({
        path: process.env.GALLERY_SCREENSHOT,
        fullPage: true,
      });
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
    await page.reload();
    await page.locator("a.scene").first().waitFor();
    assert.equal(await page.locator("#launch-form").count(), 0);
    await page.goto(builderURL);
    await page.locator("#launch-form").waitFor();
    assert.equal(
      new URL(
        await page
          .getByRole("link", { name: "Back to examples", exact: true })
          .getAttribute("href"),
        page.url(),
      ).pathname,
      prefix + "gallery/",
    );
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
    await page.locator('[name="fov"]').fill("45.5");
    await page.locator('[name="frame"]').selectOption("0");
    await page.locator('[name="embed"]').check();
    await page.locator('[name="orientation"]').selectOption("supersplat");
    assert(await page.locator('[name="up"]').isDisabled());
    await page
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    const configured = new URL(await page.locator("#launch-url").inputValue());
    assert.equal(
      configured.searchParams.get("url"),
      "https://assets.test/example.sog?key=a+b&x=1",
    );
    assert.equal(configured.searchParams.get("orientation"), "supersplat");
    assert.equal(configured.searchParams.has("up"), false);
    assert.equal(configured.searchParams.get("embed"), "1");
    assert.equal(configured.searchParams.get("frame"), "0");
    assert.equal(configured.searchParams.get("fov"), "45.5");
    await page.locator('[name="eye"]').fill("0x4,3,6");
    await page
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    assert.match(
      await page.locator("#launch-error").textContent(),
      /coordinates/,
    );
    assert.equal(
      await page.locator("#launch-url").inputValue(),
      configured.href,
    );
    await page.locator('[name="eye"]').fill("4,3,6");
    assert.equal(configured.searchParams.get("format"), "auto");
    await page
      .locator('[name="url"]')
      .fill("https://assets.test/download?key=a+b");
    await page
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    assert.match(
      await page.locator("#launch-error").textContent(),
      /select its data format/,
    );
    assert.equal(
      await page.locator("#launch-url").inputValue(),
      configured.href,
    );
    await page.locator('[name="format"]').selectOption("sog-unbundled");
    await page
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    const explicit = new URL(await page.locator("#launch-url").inputValue());
    assert.equal(explicit.searchParams.get("format"), "sog-unbundled");
    assert.equal(
      explicit.searchParams.get("url"),
      "https://assets.test/download?key=a+b",
    );
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
    if (process.env.GALLERY_SCREENSHOT)
      await page.screenshot({
        path: path.join(
          path.dirname(process.env.GALLERY_SCREENSHOT),
          "link-builder-mobile.png",
        ),
        fullPage: true,
      });
    await page.reload();
    await page.locator("#launch-form").waitFor();
    await page.goto(pointcloudBuilderURL);
    assert.equal(await page.locator('[name="viewer"]').inputValue(), "pointcloud");
    await page.locator('[name="url"]').fill("https://assets.test/potree/?key=a+b&x=%2Fy");
    await page.locator('[name="pointcloud-up"]').selectOption("z");
    assert(await page.locator('[name="embed"]').isDisabled());
    assert(!(await page.locator('[name="embed"]').isVisible()));
    await page.getByRole("button", { name: "Build viewer link", exact: true }).click();
    const cloud = new URL(await page.locator("#launch-url").inputValue());
    assert.equal(cloud.pathname, prefix + "examples/dimensify/pointcloud_viewer/");
    assert.deepEqual([...cloud.searchParams], [
      ["source", "https://assets.test/potree/?key=a+b&x=%2Fy"], ["up", "z"],
    ]);
    assert.match(await page.locator("#launch-embed").inputValue(), /title="Dimensify point cloud viewer"/);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
      "Point cloud launcher overflows mobile width");
    const secure = await browser.newPage();
    await secure.route("https://gallery.test/**", async (route) => {
      const response = await route.fetch({
        url: origin + new URL(route.request().url()).pathname,
      });
      await route.fulfill({ response });
    });
    await secure.goto(
      "https://gallery.test" + prefix + "gallery/build-link.html",
    );
    await secure.locator('[name="url"]').fill("http://remote.test/scene.sog");
    await secure
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    assert.match(
      await secure.locator("#launch-error").textContent(),
      /HTTPS scene URL/,
    );
    await secure
      .locator('[name="url"]')
      .fill("http://localhost:8080/scene.sog");
    await secure
      .getByRole("button", { name: "Build viewer link", exact: true })
      .click();
    assert.equal(
      new URL(
        await secure.locator("#launch-url").inputValue(),
      ).searchParams.get("url"),
      "http://localhost:8080/scene.sog",
    );
    await secure.close();
    const noScript = await browser.newPage({ javaScriptEnabled: false });
    await noScript.goto(origin + prefix + "splat-viewer/");
    assert(
      await noScript
        .getByText("This viewer requires JavaScript and WebGPU.", {
          exact: true,
        })
        .isVisible(),
    );
    assert(!(await noScript.locator("#startup-status").isVisible()));
    await noScript.close();
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
