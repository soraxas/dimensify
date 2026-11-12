// Real toolbar input and rendered pixels for the shared infinite ground grid.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { PNG } = require('pngjs');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const { softwareFlags } = require('./visual-browser.cjs');
function difference(a, b) {
  a = PNG.sync.read(a); b = PNG.sync.read(b);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4)
    if ([0, 1, 2].some(c => Math.abs(a.data[i + c] - b.data[i + c]) > 8)) changed++;
  return changed / (a.width * a.height);
}
(async () => {
  const [site, out = 'dist/viewer-grids'] = process.argv.slice(2);
  assert(site, 'Usage: node scripts/check-viewer-grids.cjs SITE_URL [EVIDENCE]');
  fs.mkdirSync(out, { recursive: true });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE, args: softwareFlags() });
  const results = [];
  try {
    for (const viewer of ['pointcloud', 'splat']) {
      const page = await browser.newPage({ viewport: { width: 1000, height: 760 } });
      const errors = [];
      page.on('pageerror', e => errors.push(String(e)));
      page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
      const url = new URL(viewer === 'pointcloud' ? 'examples/dimensify/pointcloud_viewer/?ui_check=1' : 'splat-viewer/?perf=1&perf_ui=1&eye=4,3,6&target=0,0,0&url=../scenes/rings.ply', site);
      const control = () => page.evaluate(viewer => viewer === 'pointcloud'
        ? window.__dimensifyPointcloudControls?.find(c => c.label.startsWith('Grid:'))
        : window.__DIMENSIFY_PERF__?.viewer_toolbar?.find(c => c.action === 'Grid'), viewer);
      await page.goto(url.href);
      await page.waitForFunction(viewer => viewer === 'pointcloud' ? window.__dimensifyPointcloudControls?.some(c => c.label === 'Grid: Off') : window.__DIMENSIFY_PERF__?.viewer_toolbar?.some(c => c.action === 'Grid'), viewer, { timeout: 120000 });
      await page.waitForTimeout(2500);
      const capture = name => page.screenshot({ path: path.join(out, `${viewer}-${name}.png`), clip: { x: 0, y: 180, width: 1000, height: 400 } });
      const before = await capture('off');
      const camera = await page.evaluate(() => window.__DIMENSIFY_PERF__?.camera);
      const click = async () => { const c = await control(); await page.mouse.click(c.x, c.y, { delay: 30 }); await page.waitForTimeout(1500); };
      await click();
      const on = await capture('on');
      const changed = difference(before, on);
      assert(changed > 0.005, `${viewer}: enabling the grid must change scene pixels (${changed})`);
      if (viewer === 'pointcloud') assert.equal((await control()).label, 'Grid: On');
      await click();
      const off = await capture('off-again');
      const restored = difference(before, off);
      assert(restored < 0.001, `${viewer}: disabling the grid must restore scene pixels (${restored})`);
      if (viewer === 'pointcloud') assert.equal((await control()).label, 'Grid: Off');
      else assert.deepEqual(await page.evaluate(() => window.__DIMENSIFY_PERF__.camera), camera);
      assert.deepEqual(errors, []);
      results.push({ viewer, changed, restored });
      await page.close();
    }
    fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({ browser: browser.version(), results }, null, 2));
    console.log('PASS: both viewer grids render, toggle off cleanly and preserve camera state');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
