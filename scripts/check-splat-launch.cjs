// Actual WebGPU startup and cross-origin iframe acceptance; captures stay in artifacts.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const http = require("node:http");
const { PNG } = require("pngjs");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const { softwareFlags } = require("./visual-browser.cjs");

(async () => {
  const [site, out = "dist/splat-launch-check"] = process.argv.slice(2);
  if (!site)
    throw Error(
      "Usage: node scripts/check-splat-launch.cjs SITE_URL [EVIDENCE]",
    );
  const base = site.endsWith("/") ? site : site + "/";
  fs.mkdirSync(out, { recursive: true });
  const fixture = path.join(__dirname, "../crates/splat/tests/fixtures");
  const requests = [];
  let embeddedUrl;
  const assets = http.createServer((req, res) => {
    const url = new URL(req.url, "http://localhost");
    if (url.pathname === "/embed") {
      res.setHeader("Content-Type", "text/html");
      res.end(
        `<html><body style="margin:0"><iframe title="Splat" style="border:0;width:100vw;height:100vh" allow="fullscreen" src="${embeddedUrl.replaceAll("&", "&amp;")}"></iframe></body></html>`,
      );
      return;
    }
    requests.push({ url: req.url, referer: req.headers.referer || null });
    if (url.pathname === "/redirect-meta") {
      res.writeHead(302, {
        Location: "/plain/meta.json",
        "Access-Control-Allow-Origin": "*",
      });
      res.end();
      return;
    }
    if (url.pathname === "/plain/huge.json") {
      res.writeHead(200, {
        "Access-Control-Allow-Origin": "*",
        "Content-Length": "1048577",
      });
      res.end(Buffer.alloc(1048577, 32));
      return;
    }
    if (url.pathname === "/plain/oversized.json") {
      res.setHeader("Access-Control-Allow-Origin", "*");
      const metadata = JSON.parse(
        fs.readFileSync(path.join(fixture, "hosted-stream/chunks/meta.json")),
      );
      metadata.count = 4294967295;
      res.end(JSON.stringify(metadata));
      return;
    }
    const file = url.pathname.startsWith("/plain/")
      ? path.join(
          fixture,
          "hosted-stream/chunks",
          url.pathname.endsWith("/download")
            ? "meta.json"
            : url.pathname.slice(7),
        )
      : url.pathname.startsWith("/stream/")
        ? path.join(
            fixture,
            "hosted-stream",
            url.pathname.endsWith("/download")
              ? "lod-meta.json"
              : url.pathname.slice(8),
          )
        : path.join(
            fixture,
            url.pathname.endsWith(".sog") || url.pathname === "/download-sog"
              ? "tiny.sog"
              : "tiny.ply",
          );
    res.setHeader("Access-Control-Allow-Origin", "*");
    if (!fs.existsSync(file)) {
      res.writeHead(404).end();
      return;
    }
    res.end(fs.readFileSync(file));
  });
  await new Promise((resolve) => assets.listen(0, "127.0.0.1", resolve));
  const origin = `http://127.0.0.1:${assets.address().port}`;
  const browser = await chromium
    .launch({
      headless: process.env.SPLAT_LAUNCH_HEADED !== "1",
      executablePath: process.env.CHROMIUM_EXECUTABLE,
      args: softwareFlags(),
    })
    .catch(async (error) => {
      await new Promise((resolve) => assets.close(resolve));
      throw error;
    });
  const errors = [],
    logs = [],
    checks = [],
    comparisons = [];
  const launch = (options) => {
    const url = new URL("splat-viewer/", base);
    url.search = new URLSearchParams(options);
    return url.href;
  };
  const watch = (page) => {
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => {
      logs.push(message.text());
      if (message.type() === "error" || message.text().includes("%cERROR%c"))
        errors.push(message.text());
    });
  };
  const waitLog = async (page, start, text) => {
    const deadline = Date.now() + 120000;
    while (!logs.slice(start).some((line) => line.includes(text))) {
      assert.deepEqual(errors, []);
      if (Date.now() > deadline)
        throw Error(
          `Timed out waiting for ${text}: ${logs.slice(-8).join("\n")}`,
        );
      await page.waitForTimeout(200);
    }
  };
  const difference = (a, b) => {
    const first = PNG.sync.read(a),
      second = PNG.sync.read(b);
    let changed = 0;
    for (let i = 0; i < first.data.length; i += 4)
      if (
        [0, 1, 2].some(
          (channel) =>
            Math.abs(first.data[i + channel] - second.data[i + channel]) > 3,
        )
      )
        changed++;
    return changed / (first.width * first.height);
  };
  try {
    let axisReference;
    for (const up of ["y", "z"]) {
      const page = await browser.newPage({
        viewport: { width: 1000, height: 760 },
      });
      watch(page);
      const start = logs.length;
      const options = {
        url: new URL("scenes/rings.ply", base).href,
        up,
        eye: "4,3,6",
        target: "0,0,0",
        mode: "orbit",
        frame: "1",
        embed: "1",
      };
      await page.goto(launch(options));
      await waitLog(page, start, "rings.ply  |  384 Gaussians");
      await page.waitForTimeout(1800);
      const image = await page.screenshot({
        path: path.join(out, `axis-${up}.png`),
      });
      const png = PNG.sync.read(image);
      let colored = 0;
      for (let i = 0; i < png.data.length; i += 4)
        if (
          Math.max(...png.data.slice(i, i + 3)) -
            Math.min(...png.data.slice(i, i + 3)) >
          65
        )
          colored++;
      assert(
        colored > 1000,
        "Embedded preset must render colored Gaussians with no toolbar",
      );
      if (axisReference)
        assert(
          difference(axisReference, image) > 0.005,
          "Source-axis query must change rendered orientation",
        );
      else axisReference = image;
      await page.mouse.move(500, 380);
      await page.mouse.down();
      await page.waitForTimeout(100);
      await page.mouse.move(650, 420, { steps: 10 });
      await page.mouse.up();
      await page.waitForTimeout(700);
      const moved = await page.screenshot({
        path: path.join(out, `axis-${up}-moved.png`),
      });
      assert(
        difference(image, moved) > 0.005,
        "Initial camera must remain interactive in embed mode",
      );
      if (up === "y") {
        const reloadStart = logs.length;
        await page.goto(launch({ ...options, frame: "0" }));
        await waitLog(page, reloadStart, "384 Gaussians");
        await page.waitForTimeout(1800);
        const unframed = await page.screenshot({
          path: path.join(out, "authored-frame-disabled.png"),
        });
        assert(
          difference(image, unframed) < 0.001,
          "Explicit starting camera must override frame=1",
        );
        checks.push("authored-camera-takes-precedence-over-framing");
        const fovStart = logs.length;
        await page.goto(launch({ ...options, frame: "0", fov: "60" }));
        await waitLog(page, fovStart, "384 Gaussians");
        await page.waitForTimeout(1800);
        const wide = await page.screenshot({
          path: path.join(out, "fov-60.png"),
        });
        assert(
          difference(unframed, wide) > 0.005,
          "FOV query must change the rendered lens",
        );
        const resetStart = logs.length;
        await page.goto(
          launch({
            ...options,
            frame: "0",
            fov: "60",
            mode: "fly",
            embed: "0",
          }),
        );
        await waitLog(page, resetStart, "384 Gaussians");
        await page.waitForTimeout(1800);
        const clip = { x: 100, y: 150, width: 800, height: 470 };
        const home = await page.screenshot({
          path: path.join(out, "authored-home.png"),
          clip,
        });
        await page.mouse.move(500, 380);
        await page.mouse.down();
        await page.waitForTimeout(100);
        await page.mouse.move(650, 420, { steps: 10 });
        await page.mouse.up();
        await page.waitForTimeout(700);
        const away = await page.screenshot({
          path: path.join(out, "authored-away.png"),
          clip,
        });
        assert(difference(home, away) > 0.005);
        await page.mouse.click(574, 46, { delay: 120 });
        await page.mouse.move(600, 380);
        await page.waitForTimeout(700);
        const reset = await page.screenshot({
          path: path.join(out, "authored-reset.png"),
          clip,
        });
        assert(
          difference(home, reset) < 0.001,
          "Reset must restore authored position, orientation and FOV",
        );
        checks.push("fov-and-authored-reset");
      }
      await page.close();
      checks.push(`axis-${up}-render-and-embedded-camera-input`);
    }
    let bundled, packedReference;
    for (const [source, format, check] of [
      [
        "/remote.sog?token=a%2Bb&x=1",
        "auto",
        "signed-cross-origin-sog-query-load",
      ],
      [
        "/stream/lod-meta.json?token=manifest-only",
        "auto",
        "streaming-worker-query-load",
      ],
      [
        "/plain/meta.json?token=plain-only",
        "auto",
        "unbundled-auto-matches-packed-lod-source",
      ],
      [
        "/plain/download?token=extensionless-only",
        "sog-unbundled",
        "unbundled-explicit-extensionless",
      ],
      [
        "/redirect-meta?token=redirect-only",
        "sog-unbundled",
        "unbundled-redirect-directory",
      ],
      ["/download-sog?token=sog-only", "sog", "bundled-explicit-extensionless"],
      ["/download-ply?token=ply-only", "ply", "ply-explicit-extensionless"],
      [
        "/stream/download?token=stream-only",
        "sog-stream",
        "lod-explicit-extensionless",
      ],
    ]) {
      const page = await browser.newPage({
        viewport: { width: 1000, height: 760 },
      });
      watch(page);
      const start = logs.length;
      const requestStart = requests.length;
      await page.goto(
        launch({
          url: origin + source,
          format,
          up: "y",
          frame: "0",
          eye: "4,3,6",
          target: "0,0,0",
          embed: "1",
        }),
      );
      await waitLog(page, start, "8 Gaussians");
      await page.waitForTimeout(1200);
      if (
        source.startsWith("/stream/") ||
        source.startsWith("/plain/") ||
        source.startsWith("/redirect-meta")
      ) {
        assert(
          await page.evaluate(
            () => window.__DIMENSIFY_SOG_CODEC__?.completed > 0,
          ),
          "Metadata sources must use the existing real worker and codec",
        );
        const children = requests
          .slice(requestStart)
          .filter((req) => new URL(req.url, origin).pathname.endsWith(".webp"));
        assert(children.length >= 5, "Source textures were not fetched");
        assert(
          children.every((req) => !req.url.includes("token=")),
          "Manifest credentials must not reach child URLs",
        );
        assert(
          children.every((req) => req.referer === null),
          "Viewer query leaked in a texture referrer",
        );
      }
      if (source.startsWith("/plain/") || source.startsWith("/redirect-meta")) {
        const metadata = requests
          .slice(requestStart)
          .filter(
            (req) => !new URL(req.url, origin).pathname.endsWith(".webp"),
          );
        assert.deepEqual(
          metadata.map((req) => req.url),
          source.startsWith("/redirect-meta") ? [source, "/plain/meta.json"] : [source],
          "Already downloaded metadata must not be fetched again",
        );
      }
      const pixels = await page.screenshot({
        path: path.join(out, `${check}.png`),
      });
      if (source.startsWith("/remote.sog")) {
        bundled = pixels;
        const image = PNG.sync.read(pixels);
        let colored = 0;
        for (let i = 0; i < image.data.length; i += 4) {
          const rgb = image.data.subarray(i, i + 3);
          if (Math.max(...rgb) - Math.min(...rgb) > 30) colored++;
        }
        assert(
          colored > 100,
          "Parity reference must contain rendered Gaussians",
        );
      }
      if (source.startsWith("/stream/lod-meta.json")) {
        packedReference = pixels;
        comparisons.push({
          name: "bundled-versus-existing-packed-renderer",
          changed_fraction: difference(bundled, pixels),
          note: "Existing packed working-set scale quantization differs from the ordinary file renderer; this is not a loader parity assertion.",
        });
      }
      if (source.startsWith("/plain/") || source.startsWith("/redirect-meta")) {
        assert(
          difference(packedReference, pixels) < 0.001,
          "Unbundled output differs from the identical source through the same packed backend",
        );
      }
      await page.close();
      checks.push(check);
    }
    assert(
      requests.some((req) => req.url === "/remote.sog?token=a%2Bb&x=1"),
      "Signed URL query was changed",
    );
    assert(
      requests
        .filter((req) => req.url.startsWith("/remote"))
        .every((req) => req.referer === null),
      "Viewer query must not leak through a referrer",
    );

    const mismatch = await browser.newPage();
    watch(mismatch);
    const mismatchRequests = requests.length;
    const mismatchStart = logs.length;
    await mismatch.goto(
      launch({ url: origin + "/plain/meta.json", format: "sog-stream" }),
    );
    await waitLog(mismatch, mismatchStart, "JSON metadata does not match");
    assert.equal(
      requests.length - mismatchRequests,
      1,
      "A type mismatch must not start texture downloads",
    );
    await mismatch.close();
    checks.push("explicit-type-mismatch-stops-before-textures");

    const huge = await browser.newPage();
    watch(huge);
    const hugeStart = logs.length;
    await huge.goto(
      launch({ url: origin + "/plain/huge.json", format: "sog-unbundled" }),
    );
    await waitLog(huge, hugeStart, "1 MiB metadata limit");
    await huge.close();
    checks.push("metadata-response-limit-before-body");
    const oversized = await browser.newPage();
    watch(oversized);
    const oversizedRequests = requests.length;
    const oversizedStart = logs.length;
    await oversized.goto(
      launch({
        url: origin + "/plain/oversized.json",
        format: "sog-unbundled",
      }),
    );
    await waitLog(oversized, oversizedStart, "Coarsest SOG coverage exceeds");
    assert.equal(
      requests.length - oversizedRequests,
      1,
      "Impossible initial admission must fail before textures",
    );
    await oversized.close();
    checks.push("impossible-admission-fails-without-textures");

    if (process.env.SPLAT_REAL_UNBUNDLED_URL) {
      const real = await browser.newPage({
        viewport: { width: 1000, height: 760 },
      });
      watch(real);
      const start = logs.length;
      await real.goto(
        launch({
          url: process.env.SPLAT_REAL_UNBUNDLED_URL,
          format: "sog-unbundled",
          up: "y",
          frame: "1",
        }),
      );
      await waitLog(real, start, "567403 Gaussians");
      await real.waitForTimeout(1200);
      await real.screenshot({ path: path.join(out, "real-unbundled.png") });
      await real.close();
      checks.push("external-unbundled-567403-splats");
    }

    const parent = await browser.newPage({
      viewport: { width: 1000, height: 760 },
    });
    watch(parent);
    embeddedUrl = launch({
      url: origin + "/remote.ply",
      embed: "1",
      frame: "1",
      mode: "orbit",
    });
    let start = logs.length;
    await parent.goto(origin + "/embed");
    await waitLog(parent, start, "8 Gaussians");
    assert.equal(parent.frames().length, 2);
    await parent.waitForTimeout(1200);
    const embedded = await parent.screenshot({
      path: path.join(out, "cross-origin-iframe.png"),
    });
    await parent.mouse.move(500, 380);
    await parent.mouse.down();
    await parent.waitForTimeout(100);
    await parent.mouse.move(650, 420, { steps: 10 });
    await parent.mouse.up();
    await parent.waitForTimeout(700);
    const embeddedMoved = await parent.screenshot({
      path: path.join(out, "cross-origin-iframe-moved.png"),
    });
    assert(
      difference(embedded, embeddedMoved) > 0.0001,
      "Cross-origin iframe must receive camera input",
    );
    await parent.close();
    checks.push("cross-origin-iframe-ply-load-and-camera-input");

    const invalid = await browser.newPage();
    watch(invalid);
    start = logs.length;
    const count = requests.length;
    await invalid.goto(launch({ url: origin + "/remote.sog", up: "invalid" }));
    await waitLog(invalid, start, "Invalid viewer link:");
    assert.equal(
      requests.length,
      count,
      "Invalid config must not start an asset request",
    );
    await invalid.screenshot({ path: path.join(out, "invalid-link.png") });
    await invalid.close();
    checks.push("invalid-options-do-not-download");
    assert.deepEqual(errors, []);
    fs.writeFileSync(
      path.join(out, "result.json"),
      JSON.stringify(
        {
          result: "PASS",
          browser: browser.version(),
          checks,
          comparisons,
          requests,
        },
        null,
        2,
      ),
    );
    console.log(JSON.stringify({ result: "PASS", checks, evidence: out }));
  } finally {
    fs.writeFileSync(path.join(out, "browser.log"), logs.join("\n"));
    await browser.close();
    await new Promise((resolve) => assets.close(resolve));
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
