// Headless acceptance of the browser robot viewer: gallery presets and URL form,
// the bundled demo, a URL-loaded URDF with package:// meshes and a failed load.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const fs = require("node:fs");
const path = require("node:path");
(async () => {
  const [origin, urdf, directory = "/tmp/dimensify-robot-browser"] =
    process.argv.slice(2);
  if (!origin || !urdf)
    throw Error(
      "Usage: node scripts/check-robot-browser.cjs SITE URDF_URL [EVIDENCE]\n" +
        "URDF_URL must load with package:// meshes, e.g. a same-site copy of a ROS package.",
    );
  fs.mkdirSync(directory, { recursive: true });
  const browser = await chromium.launch({
    // CI runs headed under xvfb with software Vulkan, like the splat launch check.
    headless: process.env.ROBOT_BROWSER_HEADED !== "1",
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: [
      "--enable-unsafe-webgpu",
      ...(process.platform === "darwin"
        ? ["--use-angle=metal"]
        : ["--enable-features=Vulkan", "--disable-vulkan-surface"]),
    ],
  });
  const context = await browser.newContext({
    viewport: { width: 1000, height: 760 },
  });
  const viewer = new URL("examples/dimensify/robosim/", origin);
  // Open a viewer page, wait for one console line and fail on unexpected errors.
  const open = async (href, expect, { failure = false } = {}) => {
    const page = await context.newPage();
    const lines = [];
    const errors = [];
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => {
      lines.push(message.text());
      if (message.type() === "error" && !failure) errors.push(message.text());
    });
    await page.goto(href);
    const deadline = Date.now() + 90000;
    while (!lines.some((line) => line.includes(expect))) {
      if (errors.length) throw Error(`${href}\n${errors.join("\n")}`);
      if (Date.now() > deadline)
        throw Error(`Timed out waiting for "${expect}" at ${href}\n${lines.slice(-8).join("\n")}`);
      await page.waitForTimeout(100);
    }
    return { page, line: lines.find((line) => line.includes(expect)) };
  };
  // The robot covers a meaningful share of the canvas once drawn and framed.
  const rendered = async (page, name) => {
    const deadline = Date.now() + 30000;
    while (true) {
      const png = await page.screenshot({ path: path.join(directory, `${name}.png`) });
      const share = await page.evaluate(async (bytes) => {
        const image = await createImageBitmap(new Blob([new Uint8Array(bytes)]));
        const canvas = new OffscreenCanvas(image.width, image.height);
        const context = canvas.getContext("2d");
        context.drawImage(image, 0, 0);
        // Ignore the overlay text in the top-left corner.
        const { data } = context.getImageData(0, 200, image.width, image.height - 200);
        let bright = 0;
        for (let index = 0; index < data.length; index += 16)
          if (data[index] + data[index + 1] + data[index + 2] > 300) bright++;
        return bright / (data.length / 16);
      }, [...png]);
      if (share > 0.01) return share;
      if (Date.now() > deadline) throw Error(`${name} did not draw the robot (${share})`);
      await page.waitForTimeout(250);
    }
  };
  try {
    const gallery = await context.newPage();
    await gallery.goto(new URL("gallery/#robots", origin).href);
    await gallery.locator('[data-filter="robots"][aria-pressed="true"]').waitFor();
    const presets = await gallery.locator("article").filter({ hasText: "Open robot" }).count();
    if (presets < 4) throw Error(`Expected robot presets, found ${presets}`);
    await gallery.screenshot({ path: path.join(directory, "gallery-robots.png"), fullPage: true });
    // The URL form opens the viewer with the exact URDF as its only option.
    await gallery.getByLabel("URDF URL").fill(urdf);
    await Promise.all([
      gallery.waitForURL(/examples\/dimensify\/robosim\/\?urdf=/),
      gallery.getByRole("button", { name: "Open in viewer" }).click(),
    ]);
    if (new URL(gallery.url()).searchParams.get("urdf") !== urdf)
      throw Error(`Form produced ${gallery.url()}`);

    const demo = await open(viewer.href, "RoboSim spawned");
    console.log(`demo: ${demo.line} (${(await rendered(demo.page, "demo")).toFixed(3)})`);

    const linked = new URL(viewer);
    linked.searchParams.set("urdf", urdf);
    const robot = await open(linked.href, "RoboSim spawned");
    if (!/visual meshes/.test(robot.line) || / 0 visual meshes/.test(robot.line))
      throw Error(`URL robot has no meshes: ${robot.line}`);
    console.log(`url: ${robot.line} (${(await rendered(robot.page, "url-robot")).toFixed(3)})`);
    // Joint keys reach the canvas: after framing settles, a held key changes the image.
    await robot.page.locator("canvas").click({ position: { x: 900, y: 700 } });
    await robot.page.waitForTimeout(1500);
    const before = await robot.page.screenshot();
    await robot.page.keyboard.press("Tab");
    await robot.page.keyboard.down("ArrowRight");
    await robot.page.waitForTimeout(800);
    await robot.page.keyboard.up("ArrowRight");
    await robot.page.waitForTimeout(300);
    const after = await robot.page.screenshot({
      path: path.join(directory, "url-robot-moved.png"),
    });
    if (before.equals(after)) throw Error("Joint keys did not change the rendered robot");

    const missing = new URL(viewer);
    missing.searchParams.set("urdf", new URL("missing/robot.urdf", origin).href);
    const failed = await open(missing.href, "Could not load robot", { failure: true });
    await failed.page.screenshot({ path: path.join(directory, "failed.png") });
    console.log(`failure: ${failed.line.slice(0, 120)}`);
    console.log(`Robot browser acceptance passed; evidence in ${directory}`);
  } finally {
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exit(1);
});
