const test = require("node:test");
const assert = require("node:assert/strict");
const { EventEmitter } = require("node:events");
const { profileSummary, startWorkerProfiling } = require("./perf-browser-cpu.cjs");
function profile() { return { nodes: [{ id: 1, callFrame: { functionName: "decode" } }, { id: 2, callFrame: { functionName: "(idle)" } }], startTime: 100, endTime: 500, samples: [1,2], timeDeltas: [100,200] }; }
class Protocol extends EventEmitter {
  constructor() { super(); this.calls = []; }
  async send(method, params) {
    this.calls.push({ method, params });
    if (method === "Target.sendMessageToTarget") {
      const command = JSON.parse(params.message);
      queueMicrotask(() => this.emit("Target.receivedMessageFromTarget", { sessionId: params.sessionId, message: JSON.stringify({ id: command.id, result: command.method === "Profiler.stop" ? { profile: profile() } : {} }) }));
    }
  }
  async detach() {}
}
function setup() {
  const session = new Protocol(), page = { exposeBinding: async (_, callback) => { page.flush = callback; }, addInitScript: async () => {} };
  return { session, page, context: { newCDPSession: async () => session } };
}
test("sample attribution retains idle and is never an OS CPU clock", () => {
  const summary = profileSummary(profile());
  assert.equal(summary.sampledProfileTimeUs, 300); assert.equal(summary.profileWindowUs, 400);
  assert.equal(summary.functions.find(x => x.functionName === "(idle)").sampledProfileTimeUs, 200);
  assert.throws(() => profileSummary({ ...profile(), timeDeltas: [-1,2] }), /Invalid/);
});
test("auto-attach pauses workers, captures before terminate and retains cancellation uncertainty", async () => {
  const fixture = setup(), controller = await startWorkerProfiling(fixture);
  fixture.session.emit("Target.attachedToTarget", { sessionId: "worker-1", targetInfo: { targetId: "one", type: "worker", url: "blob:codec" } });
  await fixture.page.flush();
  fixture.session.emit("Target.detachedFromTarget", { sessionId: "worker-1" });
  const result = await controller.stop();
  assert.equal(result.exactWorkerThreadCpuMs, null); assert.equal(result.parentTerminationRequests, 1);
  assert.equal(result.workers[0].segments[0].summary.samples, 2);
  assert.equal(result.workers[0].detached, true); assert.equal(result.workers[0].unflushedTail, true);
  assert.match(result.workers[0].completionStatus, /cancelled/);
  assert.equal(fixture.session.calls.find(x => x.method === "Target.setAutoAttach").params.waitForDebuggerOnStart, true);
  const methods = fixture.session.calls.filter(x => x.method === "Target.sendMessageToTarget").map(x => JSON.parse(x.params.message).method);
  assert.ok(methods.indexOf("Profiler.start") < methods.indexOf("Runtime.runIfWaitingForDebugger"));
});
test("retention limits cannot become successful CPU measurements", async () => {
  const fixture = setup(), controller = await startWorkerProfiling({ ...fixture, maxSamples: 1 });
  fixture.session.emit("Target.attachedToTarget", { sessionId: "worker-1", targetInfo: { targetId: "one", type: "worker", url: "blob:codec" } });
  await fixture.page.flush();
  const result = await controller.stop();
  assert.equal(result.sampleLimitReached, true); assert.equal(result.workers[0].segments.length, 0);
  assert.equal(result.exactWorkerThreadCpuMs, null);
});
test("worker lost before flush retains missing coverage without invented samples", async () => {
  const fixture = setup(), controller = await startWorkerProfiling(fixture);
  fixture.session.emit("Target.attachedToTarget", { sessionId: "worker-1", targetInfo: { targetId: "one", type: "worker", url: "blob:codec" } });
  await new Promise(resolve => setImmediate(resolve));
  fixture.session.emit("Target.detachedFromTarget", { sessionId: "worker-1" });
  const result = await controller.stop();
  assert.equal(result.workers[0].segments.length, 0);
  assert.equal(result.workers[0].unflushedTail, true);
  assert.equal(result.exactWorkerThreadCpuMs, null);
});
test("actual headless dedicated worker yields profiles before parent termination", { skip: process.env.DIMENSIFY_TEST_BROWSER !== "1" }, async () => {
  const { chromium } = require(process.env.PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext(), page = await context.newPage();
    const controller = await startWorkerProfiling({ page, context });
    await page.goto("data:text/html,<title>Worker probe</title>");
    await page.evaluate(() => {
      window.probe = new Worker(URL.createObjectURL(new Blob(["onmessage=()=>{const start=performance.now();while(performance.now()-start<150){Math.sqrt(Math.random())}postMessage('done')}"])));
      window.done = false;
      probe.onmessage = () => { probe.terminate(); window.done = true; };
      probe.postMessage(1);
    });
    await page.waitForFunction(() => window.done);
    await page.waitForTimeout(100);
    const report = await controller.stop();
    assert.equal(report.parentTerminationRequests, 1);
    assert.equal(report.workers.length, 1);
    assert.ok(report.workers[0].segments.some(segment => segment.summary.samples > 0));
    assert.equal(report.exactWorkerThreadCpuMs, null);
  } finally { await browser.close(); }
});
