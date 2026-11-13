import test, { before, after } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
let browser;
before(async () => {
  browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE });
});
after(async () => { await browser?.close(); });

async function builder(t, restored = false, query = "") {
  const page = await browser.newPage();
  t.after(() => page.close());
  await page.route("https://gallery.test/**", async route => {
    const name = new URL(route.request().url()).pathname.split("/").at(-1);
    if (!["build-link.html", "build-link.mjs", "launch.mjs", "gallery.css", "index.html", "gallery.mjs", "catalog.mjs"].includes(name)) {
      return route.abort();
    }
    let body = await readFile(new URL(`../site/gallery/${name}`, import.meta.url), "utf8");
    // Simulate the browser restoring a select before its module initializes.
    if (name === "build-link.mjs" && restored) {
      body = `document.querySelector('[name="orientation"]').value = "custom";\n` + body;
    }
    await route.fulfill({ body, contentType: name.endsWith("mjs") ? "text/javascript" : name.endsWith("css") ? "text/css" : "text/html" });
  });
  await page.goto("https://gallery.test/project/gallery/build-link.html" + query);
  return page;
}

test("custom axis restored before module initialization is visible and submitted", async t => {
  const page = await builder(t, true);
  assert.equal(await page.locator("#custom-axis").isVisible(), true);
  assert.equal(await page.locator('[name="up"]').isEnabled(), true);
  await page.locator('[name="url"]').fill("https://assets.test/scene.ply");
  await page.locator('[name="up"]').selectOption("-y");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  const link = new URL(await page.locator("#launch-url").inputValue());
  assert.equal(link.searchParams.get("up"), "-y");
  assert.equal(link.searchParams.has("orientation"), false);
});

test("pageshow synchronizes restored custom and preset selections", async t => {
  const page = await builder(t);
  for (const orientation of ["custom", "supersplat"]) {
    await page.evaluate(value => {
      document.querySelector('[name="orientation"]').value = value;
      window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
    }, orientation);
    assert.equal(await page.locator("#custom-axis").isVisible(), orientation === "custom");
    assert.equal(await page.locator('[name="up"]').isEnabled(), orientation === "custom");
  }
});

test("auto-format rejection explains extensionless and unsupported extensions", async t => {
  const page = await builder(t);
  for (const asset of ["scene", "scene.bin"]) {
    await page.locator('[name="url"]').fill(`https://assets.test/${asset}`);
    await page.getByRole("button", { name: "Build viewer link" }).click();
    assert.match(await page.locator("#launch-error").textContent(), /extensionless.*unsupported extension/i);
    await page.locator('[name="format"]').selectOption("sog-unbundled");
    await page.getByRole("button", { name: "Build viewer link" }).click();
    assert.equal(await page.locator("#launch-result").isVisible(), true);
    await page.locator('[name="format"]').selectOption("auto");
  }
});

test("pointcloud preset builds PLY and Potree URLs and an accurately labelled iframe", async t => {
  const page = await builder(t, false, "?viewer=pointcloud");
  assert.equal(await page.locator('[name="viewer"]').inputValue(), "pointcloud");
  for (const name of ["format", "orientation", "up", "eye", "target", "fov", "mode", "frame", "transparency", "embed"]) {
    assert.equal(await page.locator(`[name="${name}"]`).isEnabled(), false, name);
    assert.equal(await page.locator(`[name="${name}"]`).isVisible(), false, name);
  }
  await page.setViewportSize({ width: 390, height: 844 });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  for (const source of ["https://assets.test/cloud.ply?key=a+b&x=%2Fy", "https://assets.test/potree/", "https://assets.test/potree/metadata.json"]) {
    await page.locator('[name="url"]').fill(source);
    await page.locator('[name="pointcloud-up"]').selectOption("z");
    await page.getByRole("button", { name: "Build viewer link" }).click();
    const link = new URL(await page.locator("#launch-url").inputValue());
    assert.equal(link.pathname, "/project/examples/dimensify/pointcloud_viewer/");
    assert.deepEqual([...link.searchParams], [["source", source], ["up", "z"]]);
    assert.equal(await page.locator("#launch-link").textContent(), "Open configured point cloud viewer");
    const iframe = await page.locator("#launch-embed").inputValue();
    const parsed = await page.evaluate(markup => {
      const frame = new DOMParser().parseFromString(markup, "text/html").querySelector("iframe");
      return { src: frame.src, title: frame.title };
    }, iframe);
    assert.deepEqual(parsed, { src: link.href, title: "Dimensify point cloud viewer" });
  }
});

test("viewer switching preserves splat settings while excluding them from pointcloud validation and output", async t => {
  const page = await builder(t);
  await page.locator('[name="url"]').fill("https://assets.test/scene.ply");
  await page.locator('[name="orientation"]').selectOption("custom");
  await page.locator('[name="up"]').selectOption("-y");
  await page.locator('[name="eye"]').fill("invalid");
  await page.locator('[name="embed"]').check();
  await page.locator('[name="viewer"]').selectOption("pointcloud");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  assert.equal(await page.locator("#launch-error").textContent(), "");
  assert.equal(new URL(await page.locator("#launch-url").inputValue()).searchParams.get("up"), "y");
  await page.locator('[name="viewer"]').selectOption("splat");
  assert.equal(await page.locator("#launch-result").isVisible(), false);
  assert.equal(await page.locator('[name="up"]').inputValue(), "-y");
  assert.equal(await page.locator('[name="up"]').isEnabled(), true);
  assert.equal(await page.locator('[name="embed"]').isChecked(), true);
  await page.getByRole("button", { name: "Build viewer link" }).click();
  assert.match(await page.locator("#launch-error").textContent(), /coordinates/);
  await page.locator('[name="eye"]').fill("1,2,3");
  await page.locator('[name="target"]').fill("0,0,0");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  const link = new URL(await page.locator("#launch-url").inputValue());
  assert.equal(link.pathname, "/project/splat-viewer/");
  assert.equal(link.searchParams.get("up"), "-y");
  assert.equal(link.searchParams.get("embed"), "1");
});

test("gallery offers a pointcloud builder preset alongside the splat builder", async t => {
  const page = await builder(t);
  await page.goto("https://gallery.test/project/gallery/index.html");
  for (const [href, viewer] of [["../splat-viewer/", "splat"], ["../examples/dimensify/pointcloud_viewer/", "pointcloud"]]) {
    const card = page.locator("article").filter({ has: page.locator(`a.live[href="${href}"]`) });
    const link = card.getByRole("link", { name: "Build a link", exact: true });
    assert.equal(await link.count(), 1);
    const destination = new URL(await link.getAttribute("href"), page.url()).href;
    await link.click();
    await page.waitForURL(destination);
    await page.waitForLoadState("load");
    assert.equal(await page.locator('[name="viewer"]').inputValue(), viewer);
    await page.goBack();
  }
});

test("pointcloud URL errors describe available controls and enforce hosted HTTPS", async t => {
  const page = await builder(t, false, "?viewer=pointcloud");
  await page.locator('[name="url"]').fill("ftp://assets.test/cloud.ply");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  const error = await page.locator("#launch-error").textContent();
  assert.match(error, /HTTP\(S\)/);
  assert.doesNotMatch(error, /data format/);
  await page.locator('[name="url"]').fill("http://assets.test/cloud.ply");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  assert.match(await page.locator("#launch-error").textContent(), /HTTPS/);
  await page.locator('[name="url"]').fill("http://localhost:8080/potree/");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  assert.equal(new URL(await page.locator("#launch-url").inputValue()).searchParams.get("source"), "http://localhost:8080/potree/");
});


test("pointcloud builder normalizes uppercase HTTPS in links and iframes without changing signed queries", async t => {
  const page = await builder(t, false, "?viewer=pointcloud");
  const suffix = "Assets.test:443/Cloud%2fScan.ply?sig=A+b%2f%2F&space=%20&space=+&flag&empty=#Part";
  await page.locator('[name="url"]').fill("HTTPS://" + suffix);
  await page.getByRole("button", { name: "Build viewer link" }).click();
  assert.equal(await page.locator("#launch-error").textContent(), "");
  const link = new URL(await page.locator("#launch-url").inputValue());
  assert.equal(link.searchParams.get("source"), "https://" + suffix);
  const iframeURL = await page.evaluate(markup =>
    new DOMParser().parseFromString(markup, "text/html").querySelector("iframe").src,
    await page.locator("#launch-embed").inputValue());
  assert.equal(iframeURL, link.href);
  assert.equal(new URL(iframeURL).searchParams.get("source"), "https://" + suffix);
});


test("pointcloud rejects unsupported source suffixes after switching from splat", async t => {
  const page = await builder(t);
  await page.locator('[name="url"]').fill("https://assets.test/scene.sog");
  await page.getByRole("button", { name: "Build viewer link" }).click();
  assert.equal(await page.locator("#launch-result").isVisible(), true);
  await page.locator('[name="viewer"]').selectOption("pointcloud");
  for (const path of ["scene.sog", "scene.SOG", "cloud.laz", "cloud.LaZ", "cloud.las", "cloud.LAS", "cloud.copc", "cloud.COPC", "cloud.js", "CLOUD.JS"]) {
    await page.locator('[name="url"]').fill(`https://assets.test/${path}?sig=a+b%2F`);
    await page.getByRole("button", { name: "Build viewer link" }).click();
    assert.equal(await page.locator("#launch-result").isVisible(), false, path);
    assert.match(await page.locator("#launch-error").textContent(), /PLY.*Potree/i, path);
  }
  for (const path of ["download", "pointcloud/", "metadata.json", "cloud.ply", "cloud.sog/", "download?filename=cloud.laz"]) {
    const source = `https://assets.test/${path}`;
    await page.locator('[name="url"]').fill(source);
    await page.getByRole("button", { name: "Build viewer link" }).click();
    assert.equal(await page.locator("#launch-error").textContent(), "", path);
    assert.equal(await page.locator("#launch-result").isVisible(), true, path);
    assert.equal(new URL(await page.locator("#launch-url").inputValue()).searchParams.get("source"), source);
  }
});
