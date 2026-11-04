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

async function builder(t, restored = false) {
  const page = await browser.newPage();
  t.after(() => page.close());
  await page.route("https://gallery.test/**", async route => {
    const name = new URL(route.request().url()).pathname.split("/").at(-1);
    if (!["build-link.html", "build-link.mjs", "launch.mjs", "gallery.css"].includes(name)) {
      return route.abort();
    }
    let body = await readFile(new URL(`../splat-web/gallery/${name}`, import.meta.url), "utf8");
    // Simulate the browser restoring a select before its module initializes.
    if (name === "build-link.mjs" && restored) {
      body = `document.querySelector('[name="orientation"]').value = "custom";\n` + body;
    }
    await route.fulfill({ body, contentType: name.endsWith("mjs") ? "text/javascript" : name.endsWith("css") ? "text/css" : "text/html" });
  });
  await page.goto("https://gallery.test/gallery/build-link.html");
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
