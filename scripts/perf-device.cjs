// Physical-device connections require operator identity plus browser observations.
const fs = require("node:fs");
const { validateTarget, validateObservedTarget, sanitize } = require("./visual-browser.cjs");
function connectionOptions(args = {}, env = process.env) {
  const cdpEndpoint = args["cdp-endpoint"] || env.DIMENSIFY_CDP_ENDPOINT;
  const wsEndpoint = args["ws-endpoint"] || env.BROWSERSTACK_WS_ENDPOINT;
  if (cdpEndpoint && wsEndpoint) throw new Error("Select exactly one remote connection mechanism");
  const remote = Boolean(cdpEndpoint || wsEndpoint);
  const target = args.target ? validateTarget(typeof args.target === "string" ? JSON.parse(fs.readFileSync(args.target, "utf8")) : args.target, remote) : null;
  if (remote && !target) throw new Error("Remote performance replay requires --target physical-device identity JSON");
  return { target, cdpEndpoint, wsEndpoint };
}
async function connectBrowser(chromium, { target = null, cdpEndpoint, wsEndpoint, timeoutMs = 180000, launchOptions = {}, contextOptions = {} } = {}) {
  const remote = Boolean(cdpEndpoint || wsEndpoint);
  if (cdpEndpoint && wsEndpoint) throw new Error("Select exactly one remote connection mechanism");
  if (remote || target) validateTarget(target, remote);
  let browser, context;
  const secrets = [cdpEndpoint, wsEndpoint];
  try {
    browser = cdpEndpoint ? await chromium.connectOverCDP(cdpEndpoint, { timeout: timeoutMs }) : wsEndpoint ? await chromium.connect(wsEndpoint, { timeout: timeoutMs }) : await chromium.launch({ ...launchOptions, timeout: timeoutMs });
    // Existing CDP contexts preserve real-device viewport, scale factor and browser configuration.
    context = cdpEndpoint ? browser.contexts()[0] : await browser.newContext(target ? {} : contextOptions);
    if (!context) throw new Error("Remote browser has no device context");
    return { browser, context, connection: cdpEndpoint ? "cdp" : wsEndpoint ? "playwright-remote" : "local", physicalDevice: target,
      isolation: cdpEndpoint ? "existing physical-device context; HTTP/application cache and GPU startup isolation unavailable" : remote ? "fresh context; remote device process and GPU startup isolation unavailable" : "fresh browser process and context",
      async close() { if (!cdpEndpoint) await context.close().catch(() => {}); await browser.close().catch(() => {}); } };
  } catch (error) {
    if (browser) await browser.close().catch(() => {});
    throw new Error(sanitize(error, secrets));
  }
}
async function observeDevice(page, target = null) {
  const observed = await page.evaluate(() => ({ userAgent: navigator.userAgent, platform: navigator.platform, maxTouchPoints: navigator.maxTouchPoints, dpr: devicePixelRatio, screen: { width: screen.width, height: screen.height, availWidth: screen.availWidth, availHeight: screen.availHeight }, inner: { width: innerWidth, height: innerHeight }, webgpuAvailable: Boolean(navigator.gpu) }));
  validateObservedTarget(target, observed);
  if (!Number.isFinite(observed.dpr) || observed.dpr <= 0 || !(observed.screen?.width > 0) || !(observed.screen?.height > 0)) throw new Error("Missing observed browser resolution/DPR");
  if (target && !(observed.maxTouchPoints > 0)) throw new Error("Physical mobile browser must report touch input");
  return { ...observed, physicalDeviceEvidence: target ? "operator declaration plus observed OS, touch, screen and DPR; browser observations cannot independently attest physical hardware or exclude a spoofed/emulated browser" : null };
}
module.exports = { connectionOptions, connectBrowser, observeDevice, sanitize };
