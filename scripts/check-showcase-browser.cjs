// Headless acceptance of the real gallery, camera playground and room viewers.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
const fs = require("node:fs");
const path = require("node:path");
(async () => {
  const [origin, endpoint, directory = "/tmp/dimensify-showcase-browser"] =
    process.argv.slice(2);
  if (!origin || !endpoint)
    throw Error(
      "Usage: node scripts/check-showcase-browser.cjs SITE WS_ENDPOINT [EVIDENCE]",
    );
  fs.mkdirSync(directory, { recursive: true });
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CHROMIUM_EXECUTABLE,
    args: [
      "--enable-unsafe-webgpu",
      ...(process.platform === "darwin"
        ? ["--use-angle=metal"]
        : ["--enable-features=Vulkan", "--disable-vulkan-surface"]),
    ],
  });
  const errors = [],
    logs = new Map();
  const context = await browser.newContext({
    viewport: { width: 1000, height: 760 },
  });
  const watch = (page) => {
    const entries = [];
    logs.set(page, entries);
    page.on("pageerror", (error) => errors.push(String(error)));
    page.on("console", (message) => {
      entries.push(message.text());
      if (message.type() === "error" || message.text().includes("%cERROR%c"))
        errors.push(message.text());
    });
  };
  context.on("page", watch);
  const waitLog = async (page, text) => {
    const deadline = Date.now() + 45000;
    while (!(logs.get(page) || []).some((line) => line.includes(text))) {
      if (errors.length) throw Error(errors.join("\n"));
      if (Date.now() > deadline)
        throw Error(
          `Timed out: ${text}\n${logs.get(page).slice(-6).join("\n")}`,
        );
      await page.waitForTimeout(100);
    }
  };
  const capture = async (page, name) =>
    page.screenshot({
      path: path.join(directory, name + ".png"),
      clip: { x: 120, y: 280, width: 760, height: 300 },
    });
  const waitScene = async (page, name) => {
    const deadline = Date.now() + 30000;
    while (true) {
      const screenshot = await capture(page, name);
      const colored = await page.evaluate(
        async (bytes) => {
          const image = await createImageBitmap(
            new Blob([new Uint8Array(bytes)], { type: "image/png" }),
          );
          const canvas = new OffscreenCanvas(image.width, image.height);
          const context = canvas.getContext("2d");
          context.drawImage(image, 0, 0);
          image.close();
          const pixels = context.getImageData(
            0,
            0,
            canvas.width,
            canvas.height,
          ).data;
          for (let index = 0; index < pixels.length; index += 16) {
            const channels = [
              pixels[index],
              pixels[index + 1],
              pixels[index + 2],
            ];
            if (Math.max(...channels) - Math.min(...channels) > 70) return true;
          }
          return false;
        },
        [...screenshot],
      );
      if (colored) return screenshot;
      if (Date.now() > deadline)
        throw Error(`Scene did not render its colored cube: ${name}`);
      await page.waitForTimeout(200);
    }
  };
  const click = async (page, x, y) => {
    await page.bringToFront();
    await page.mouse.click(x, y, { delay: 120 });
    await page.waitForTimeout(500);
  };
  const drag = async (page) => {
    await page.mouse.move(500, 350);
    await page.mouse.down();
    await page.waitForTimeout(100);
    await page.mouse.move(620, 400, { steps: 10 });
    await page.mouse.up();
    await page.waitForTimeout(500);
  };
  try {
    const camera = await context.newPage();
    await camera.goto(new URL("gallery/", origin).href);
    await camera.getByRole("button", { name: "Browser", exact: true }).click();
    await camera.screenshot({
      path: path.join(directory, "gallery-browser.png"),
      fullPage: true,
    });
    await camera
      .locator("article")
      .filter({
        has: camera.getByRole("heading", {
          name: "Camera playground",
          exact: true,
        }),
      })
      .getByRole("link", { name: "Open browser example", exact: true })
      .click();
    await camera.locator("canvas").waitFor();
    await waitLog(camera, "Shared camera playground ready");
    await camera.waitForTimeout(1500);
    const initial = await capture(camera, "camera-start");
    await drag(camera);
    const orbit = await capture(camera, "camera-orbit");
    if (initial.equals(orbit))
      throw Error("Playground orbit did not render movement");
    await click(camera, 180, 46);
    const reset = await capture(camera, "camera-reset");
    if (orbit.equals(reset))
      throw Error("Playground Reset did not change the camera");
    await click(camera, 290, 46);
    await camera.mouse.move(500, 350);
    await camera.keyboard.down("w");
    await camera.waitForTimeout(600);
    await camera.keyboard.up("w");
    await camera.waitForTimeout(400);
    const fly = await capture(camera, "camera-fly");
    if (reset.equals(fly)) throw Error("Playground Fly did not move");
    await click(camera, 180, 46);
    await click(camera, 290, 46);
    const beforeTouch = await capture(camera, "camera-before-touch");
    const cdp = await context.newCDPSession(camera);
    await cdp.send("Emulation.setTouchEmulationEnabled", {
      enabled: true,
      maxTouchPoints: 5,
    });
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchStart",
      touchPoints: [{ id: 1, x: 450, y: 350 }],
    });
    await camera.waitForTimeout(100);
    for (let i = 1; i <= 8; i++) {
      await cdp.send("Input.dispatchTouchEvent", {
        type: "touchMove",
        touchPoints: [{ id: 1, x: 450 + i * 15, y: 350 + i * 4 }],
      });
      await camera.waitForTimeout(40);
    }
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchEnd",
      touchPoints: [],
    });
    await camera.waitForTimeout(500);
    if (beforeTouch.equals(await capture(camera, "camera-touch")))
      throw Error("Playground touch did not orbit");
    await cdp.detach();
    await camera.close();

    const host = await context.newPage();
    const target = new URL(
      "examples/dimensify_session/browser_collaboration/",
      origin,
    );
    // Start with invalid configuration so the connection form must establish the room.
    target.searchParams.set("endpoint", "invalid");
    await host.goto(target.href);
    await waitLog(host, "Enter a ws://");
    await host.waitForTimeout(1500);
    const fill = async (page, y, value) => {
      await click(page, 160, y);
      await page.keyboard.press(
        process.platform === "darwin" ? "Meta+A" : "Control+A",
      );
      await page.waitForTimeout(100);
      await page.keyboard.type(value, { delay: 20 });
      await page.waitForTimeout(150);
    };
    await fill(host, 122, endpoint);
    await fill(host, 210, "owner");
    await click(host, 80, 352);
    await waitLog(host, "participants=1 revision=0");
    console.log("Connection form created a room");
    await host.waitForTimeout(1500);
    const editorPromise = context.waitForEvent("page");
    await click(host, 450, 126);
    const editor = await editorPromise;
    await waitLog(editor, "participants=2 revision=0");
    await waitLog(host, "participants=2 revision=0");
    const beforeEdit = await capture(host, "cube-start");
    await click(host, 80, 126);
    await waitLog(host, "participants=2 revision=1");
    await waitLog(editor, "participants=2 revision=1");
    if (beforeEdit.equals(await capture(host, "cube-color")))
      throw Error("Acknowledged edit did not change rendered cube");
    await click(editor, 205, 126);
    await waitLog(host, "participants=2 revision=2");
    await waitLog(editor, "participants=2 revision=2");
    await capture(host, "host-moved");
    await capture(editor, "editor-moved");
    const viewerPromise = context.waitForEvent("page");
    await click(host, 575, 126);
    const viewer = await viewerPromise;
    await waitLog(viewer, "| Viewer |");
    await waitScene(viewer, "viewer-visible");
    await waitLog(host, "participants=3 revision=2");
    await click(viewer, 80, 126);
    await viewer.waitForTimeout(800);
    const last = (logs.get(viewer) || [])
      .filter((line) => line.includes("COLLAB"))
      .at(-1);
    if (!last.includes("revision=2") || !last.includes("Error: Unauthorized"))
      throw Error("Viewer edit was not rejected");
    await waitScene(viewer, "viewer-read-only");
    await host.screenshot({
      path: path.join(directory, "collaboration-desktop.png"),
    });
    await viewer.setViewportSize({ width: 390, height: 844 });
    await viewer.waitForTimeout(800);
    await viewer.screenshot({
      path: path.join(directory, "collaboration-mobile.png"),
    });
    await viewer.setViewportSize({ width: 1000, height: 760 });
    await click(editor, 330, 126);
    await drag(editor);
    const exploring = await capture(editor, "editor-exploring");
    await click(editor, 330, 126);
    await editor.waitForTimeout(700);
    const following = await capture(editor, "editor-following");
    if (exploring.equals(following))
      throw Error("Follow did not restore the presenter view");
    // Invalid input must preserve the current room; typing must not move its camera.
    await host.bringToFront();
    const beforeForm = await capture(host, "before-connection-form");
    await click(host, 700, 126);
    await fill(host, 122, "https://invalid-host");
    await click(host, 190, 352);
    await waitLog(host, "Enter a ws://");
    await host.screenshot({
      path: path.join(directory, "connection-invalid.png"),
    });
    await host.keyboard.press("Escape");
    await host.waitForTimeout(500);
    if (!beforeForm.equals(await capture(host, "after-invalid-connection")))
      throw Error(
        "Form typing changed the camera or replaced the active document",
      );
    const lastHost = logs
      .get(host)
      .filter((line) => line.includes("COLLAB"))
      .at(-1);
    if (!lastHost.includes("participants=3 revision=2"))
      throw Error("Invalid form replaced the active room");
    // Retry ignores unsubmitted invalid text and resumes the same room and owner.
    const reconnectStart = logs.get(host).length;
    await click(host, 700, 126);
    await click(host, 298, 352);
    const retryDeadline = Date.now() + 30000;
    while (
      !logs
        .get(host)
        .slice(reconnectStart)
        .some((line) => line.includes("Presenting | participants=3 revision=2"))
    ) {
      if (Date.now() > retryDeadline)
        throw Error("Retry did not resume the original room owner");
      await host.waitForTimeout(100);
    }
    await waitScene(host, "retry-resumed");
    console.log("Retry resumed the original room owner");
    const invitation = logs
      .get(host)
      .join("\n")
      .match(/Editor ([A-Z0-9-]{14})/)[1];
    // A separate page joins through form fields, without endpoint/code URL parameters.
    const joiner = await context.newPage();
    await joiner.goto(target.href);
    await waitLog(joiner, "Enter a ws://");
    await joiner.waitForTimeout(1500);
    await fill(joiner, 122, endpoint);
    await fill(joiner, 210, "form guest");
    await fill(joiner, 298, invitation);
    await click(joiner, 190, 352);
    await waitLog(joiner, "participants=4 revision=2");
    await waitLog(host, "participants=4 revision=2");
    await waitScene(joiner, "form-joined");
    await click(joiner, 700, 126);
    await joiner.screenshot({
      path: path.join(directory, "connection-desktop.png"),
    });
    await joiner.setViewportSize({ width: 390, height: 844 });
    await joiner.waitForTimeout(700);
    await joiner.screenshot({
      path: path.join(directory, "connection-mobile.png"),
    });
    if (errors.length) throw Error(errors.join("\n"));
    console.log(
      JSON.stringify({
        result: "PASS",
        browser: browser.version(),
        checks: [
          "camera orbit/reset/fly",
          "real touch camera input",
          "two tabs join by invite",
          "acknowledged color/move edits render",
          "three participants present",
          "viewer edit rejected",
          "explore/follow handoff",
          "create and join through retained connection fields",
          "invalid form preserves room and camera",
          "retry resumes the same room owner",
          "desktop/mobile connection layout",
        ],
        evidence: directory,
      }),
    );
  } finally {
    let index = 0;
    for (const page of context.pages()) {
      await page
        .screenshot({ path: path.join(directory, `final-page-${index++}.png`) })
        .catch(() => {});
    }
    index = 0;
    for (const entries of logs.values())
      fs.writeFileSync(
        path.join(directory, `page-${index++}.log`),
        entries.join("\n"),
      );
    await browser.close();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
