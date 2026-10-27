// Requires Playwright and an installed Chromium browser. Always runs headlessly.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");

(async () => {
  const url = process.argv[2];
  if (!url)
    throw new Error(
      "Usage: node scripts/check-browser.cjs http://127.0.0.1:8000",
    );
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CHROMIUM_EXECUTABLE,
  });
  try {
    const page = await browser.newPage();
    page.on("pageerror", (error) => console.error(error));
    await page.goto(url);
    await page.waitForFunction(
      () => typeof globalThis.COLLAB_RESULT === "string",
      {},
      { timeout: 45000 },
    );
    const result = await page.evaluate(() => globalThis.COLLAB_RESULT);
    console.log(JSON.stringify({ browser: browser.version(), url, result }));
    if (!result.startsWith("PASS:")) throw new Error(result);
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
