// Real canvas input only. Fixed viewport and gesture sequence are recorded.
const defaultReplay = [
  { name: "static-main", action: "hold" },
  { name: "moving-main", action: "turn-return" },
  { name: "pan-return", action: "pan-return" },
  { name: "near-side", action: "near-side" },
  { name: "static-near-side", action: "hold" },
  { name: "moving-near-side", action: "turn-return" },
  { name: "return", action: "frame" },
];
function validateReplay(steps) {
  if (!Array.isArray(steps) || !steps.length || steps.length > 32) throw new Error("Replay requires 1..32 steps");
  const names = new Set();
  for (const step of steps) {
    if (!/^[a-z0-9-]+$/.test(step.name || "") || names.has(step.name) || !["hold", "turn-return", "pan-return", "near-side", "frame"].includes(step.action)) throw new Error("Invalid replay step");
    names.add(step.name);
  }
  return steps;
}
async function gesture(page, button, dx, dy, durationMs) {
  const x = 600, y = 380, ticks = 30;
  await page.mouse.move(x, y);
  await page.mouse.down({ button });
  for (let i = 1; i <= ticks; i++) {
    await page.mouse.move(x + dx * i / ticks, y + dy * i / ticks);
    await page.waitForTimeout(durationMs / ticks);
  }
  await page.mouse.up({ button });
}
async function frameHome(page) {
  await page.mouse.click(574, 46, { delay: 120 });
  await page.waitForTimeout(150);
  await page.mouse.click(450, 46, { delay: 120 });
  await page.mouse.move(600, 380);
}
async function perform(page, action, durationMs) {
  if (action === "hold") return page.waitForTimeout(durationMs);
  if (action === "frame") { await frameHome(page); return page.waitForTimeout(durationMs); }
  if (action === "near-side") {
    await gesture(page, "left", 150, 25, durationMs / 2);
    await page.mouse.wheel(0, -200);
    return page.waitForTimeout(durationMs / 2);
  }
  const button = action === "pan-return" ? "right" : "left";
  await gesture(page, button, 100, 20, durationMs / 2);
  await gesture(page, button, -100, -20, durationMs / 2);
}
async function observeDuring(work, snapshot, intervalMs = 100) {
  let done = false;
  const task = Promise.resolve().then(work).finally(() => { done = true; });
  // Attach a rejection handler immediately, including when snapshot is pending.
  const completion = task.then(() => ({ rejected: false }), (error) => ({ rejected: true, error }));
  let observationFailed = false;
  let observationError;
  try {
    while (!done) {
      await snapshot();
      if (!done) await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
  } catch (error) { observationFailed = true; observationError = error; }
  const workResult = await completion;
  if (observationFailed) throw observationError;
  if (workResult.rejected) throw workResult.error;
}
module.exports = { defaultReplay, validateReplay, perform, frameHome, observeDuring };
