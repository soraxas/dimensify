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
  const types = { ".html": "text/html", ".mjs": "text/javascript", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm" };
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://localhost");
    const file = path.resolve(root, decodeURIComponent(url.pathname.slice(prefix.length)));
    if (!url.pathname.startsWith(prefix) || !file.startsWith(root + path.sep) && file !== root) {
      res.writeHead(404).end(); return;
    }
    const source = fs.existsSync(file) && fs.statSync(file).isDirectory() ? path.join(file, "index.html") : file;
    if (!fs.existsSync(source) || !fs.statSync(source).isFile()) { res.writeHead(404).end(); return; }
    res.writeHead(200, { "Content-Type": types[path.extname(source)] || "application/octet-stream" });
    fs.createReadStream(source).pipe(res);
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  let browser;
  try {
    const origin = `http://127.0.0.1:${server.address().port}`;
    browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE });
    const page = await browser.newPage();
    const errors = [];
    page.on("pageerror", error => errors.push(String(error)));
    page.on("response", response => { if (response.status() >= 400) errors.push(`${response.status()} ${response.url()}`); });
    await page.goto(origin + prefix);
    await page.waitForURL("**/gallery/");
    await page.locator("#examples article").first().waitFor();
    const count = await page.locator("#examples article").count();
    assert(count > 3, "Expected browser and local examples");
    await page.getByRole("button", { name: "Browser", exact: true }).click();
    const live = await page.locator("#examples a.live").evaluateAll(links => links.map(link => link.href));
    assert.equal(live.length, 3, "Expected splat, camera and collaboration examples");
    for (const href of live) {
      assert(new URL(href).pathname.startsWith(prefix), `Escaped project base path: ${href}`);
      const response = await page.request.get(href);
      assert.equal(response.status(), 200, href);
      const markup = await response.text();
      assert(markup.includes("dimensify-canvas"), `Missing canvas in ${href}`);
      const module = markup.match(/import init from ["']([^"']+)["']/);
      assert(module, `Missing WASM loader in ${href}`);
      const loader = new URL(module[1], href).href;
      const js = await page.request.get(loader);
      assert.equal(js.status(), 200, "Missing JavaScript bindings");
      // Import without init: resolve real snippet dependencies without a GPU.
      await page.evaluate(url => import(url).then(() => true), loader);
    }
    if (process.env.GALLERY_SCREENSHOT)
      await page.screenshot({ path: process.env.GALLERY_SCREENSHOT, fullPage: true });
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "Gallery overflows mobile width");
    await page.getByRole("button", { name: "Headless API / server" }).click();
    assert(await page.locator("#examples article").count() > 0, "Missing headless examples");
    assert.equal(await page.locator("#examples a.live").count(), 0, "Headless examples must not have browser launch links");
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({ browser: browser.version(), examples: count, browserExamples: live.length, projectSubpath: prefix, result: "PASS" }));
  } finally {
    if (browser) await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
