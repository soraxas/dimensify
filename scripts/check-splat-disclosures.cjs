// Real-input checks of clipped layout animation, reversal and reduced motion.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const { softwareFlags } = require("./visual-browser.cjs");

(async () => {
  const [site, out = "dist/splat-disclosures"] = process.argv.slice(2);
  if (!site) throw Error("Usage: node scripts/check-splat-disclosures.cjs VIEWER_URL [EVIDENCE]");
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE, args: softwareFlags() });
  const results = [];
  try {
    for (const reduced of [false, true]) {
      const context = await browser.newContext({ viewport: { width: 1000, height: 760 }, reducedMotion: reduced ? "reduce" : "no-preference" });
      const page = await context.newPage();
      const errors = [];
      page.on("pageerror", error => errors.push(String(error)));
      page.on("console", message => {
        if (message.type() === "error") errors.push(message.text());
      });
      const url = new URL(site);
      url.searchParams.set("perf", "1");
      url.searchParams.set("perf_ui", "1");
      await page.goto(url.href);
      await page.waitForFunction(() => window.__DIMENSIFY_PERF__?.viewer_disclosures, null, { timeout: 120000 });
      const state = () => page.evaluate(() => window.__DIMENSIFY_PERF__);
      assert.equal((await state()).viewer_reduce_motion, reduced);
      assert.deepEqual((await state()).viewer_toolbar.map(c => c.action).sort(), ["Frame", "Grid", "Mode", "Open", "Reset", "Settings", "Tour", "Url"]);
      const click = async control => page.mouse.click(control.x, control.y, { delay: 15 });
      await click((await state()).viewer_toolbar.find(c => c.action === "Settings"));
      await page.waitForTimeout(300);
      const header = () => state().then(s => s.viewer_controls.find(c => c.field === "Section(Coordinates)"));
      const sample = () => page.evaluate(() => new Promise(resolve => {
        const values = [], started = performance.now();
        function frame() {
          const row = window.__DIMENSIFY_PERF__.viewer_disclosures.find(c => c.section === "Coordinates");
          const state = window.__DIMENSIFY_PERF__;
          values.push({ at: performance.now() - started, height: row.height,
            observedAt: state.observed_at_ms, appFrame: state.app_frames?.total,
            maxAppFrameIntervalMs: Math.max(0, ...(state.app_frames?.intervals ?? [])
              .filter(interval => interval[1] >= performance.timeOrigin + started)
              .map(interval => interval[2])) });
          if (performance.now() - started < 450) requestAnimationFrame(frame); else resolve(values);
        }
        frame();
      }));
      const transition = async () => {
        // Start collecting before input: a slow first frame may finish the
        // transition before a collector started after the click can observe it.
        const pending = sample();
        await click(await header());
        return pending;
      };
      const camera = (await state()).camera;
      const opening = await transition();
      const full = opening.at(-1).height;
      assert(full > 100, "Opening must reveal real section content");
      const closing = await transition();
      assert.equal(closing.at(-1).height, 0, "Closed section must leave no content height");
      const cadence = values => {
        const updates = values.filter((v, i) => !i || v.appFrame !== values[i - 1].appFrame);
        return {
          maxRafGapMs: Math.max(0, ...values.slice(1).map((v, i) => v.at - values[i].at)),
          maxAppFrameIntervalMs: Math.max(0, ...values.map(v => v.maxAppFrameIntervalMs)),
          maxObservedAppGapMs: Math.max(0, ...updates.slice(1).map((v, i) => v.observedAt - updates[i].observedAt)),
          appUpdates: updates.length,
        };
      };
      const frameCadence = { opening: cadence(opening), closing: cadence(closing) };
      fs.writeFileSync(path.join(out, reduced ? "reduced-motion-samples.json" : "animated-samples.json"), JSON.stringify({ opening, closing, frameCadence, errors }, null, 2));
      const partial = values => values.some(v => v.height > 1 && v.height < full - 1);
      if (reduced) {
        assert(!partial(opening) && !partial(closing), "Reduced motion must skip intermediate layout heights");
      } else {
        assert(partial(opening) && partial(closing),
          `Opening and closing must render intermediate layout heights. Frame cadence: ${JSON.stringify(frameCadence)}. ` +
          "A frame/update gap at or above the 200 ms transition can hide intermediate heights; this run does not establish animation coverage. Inspect the saved samples and rerun on a responsive renderer.");
        await click(await header());
        await page.waitForFunction(max => {
          const h = window.__DIMENSIFY_PERF__.viewer_disclosures.find(c => c.section === "Coordinates").height;
          return h > 1 && h < max - 1;
        }, full, { timeout: 5000, polling: "raf" });
        await click(await header());
        const reversed = await sample();
        assert.equal(reversed.at(-1).height, 0, "Reversing a transition must settle closed");
      }
      await page.emulateMedia({ reducedMotion: reduced ? "no-preference" : "reduce" });
      await page.waitForFunction(expected => window.__DIMENSIFY_PERF__.viewer_reduce_motion === expected,
        !reduced, { timeout: 5000 });
      await page.emulateMedia({ reducedMotion: reduced ? "reduce" : "no-preference" });
      await page.waitForFunction(expected => window.__DIMENSIFY_PERF__.viewer_reduce_motion === expected,
        reduced, { timeout: 5000 });
      assert.deepEqual((await state()).camera, camera, "Disclosure interaction moved the camera");
      assert.deepEqual(errors, []);
      await page.screenshot({ path: path.join(out, reduced ? "reduced-motion.png" : "animated.png") });
      results.push({ reduced, fullHeight: full, opening, closing, frameCadence });
      await context.close();
    }
    fs.writeFileSync(path.join(out, "result.json"), JSON.stringify({ result: "PASS", browser: browser.version(), results }, null, 2));
    console.log("PASS: animated disclosures, reversal, reduced motion and eight-action toolbar");
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
