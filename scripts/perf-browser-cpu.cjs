// Opt-in diagnostic cohort: V8 sampled stack profiles, never OS thread CPU time.
// Protocol references: ChromeDevTools/devtools-protocol json/{js,browser}_protocol.json.
const BINDING = "__dimensifyFlushWorkerProfiles";
function profileSummary(profile) {
  if (!profile || !Number.isFinite(profile.startTime) || !Number.isFinite(profile.endTime) || profile.endTime < profile.startTime || !Array.isArray(profile.nodes) || !Array.isArray(profile.samples) || !Array.isArray(profile.timeDeltas) || profile.samples.length !== profile.timeDeltas.length || profile.timeDeltas.some(x => !Number.isFinite(x) || x < 0)) throw new Error("Invalid V8 sampled profile");
  const nodes = new Map(profile.nodes.map(node => [node.id, node]));
  const byFunction = new Map();
  for (let i = 0; i < profile.samples.length; i++) {
    const node = nodes.get(profile.samples[i]);
    if (!node) throw new Error("Profile sample references missing node");
    const name = node.callFrame?.functionName || "(anonymous)";
    const value = byFunction.get(name) || { functionName: name, samples: 0, sampledProfileTimeUs: 0 };
    value.samples++; value.sampledProfileTimeUs += profile.timeDeltas[i]; byFunction.set(name, value);
  }
  return { samples: profile.samples.length, profileWindowUs: profile.endTime - profile.startTime, sampledProfileTimeUs: profile.timeDeltas.reduce((a,b) => a+b, 0), functions: [...byFunction.values()] };
}
async function startWorkerProfiling({ page, context, samplingIntervalUs = 1000, maxWorkers = 256, maxSamples = 250000, maxNodes = 50000, commandTimeoutMs = 3000 }) {
  if (![samplingIntervalUs, maxWorkers, maxSamples, maxNodes, commandTimeoutMs].every(Number.isInteger) || samplingIntervalUs < 100 || maxWorkers < 1 || maxSamples < 1 || maxNodes < 1 || commandTimeoutMs < 100) throw new Error("Invalid worker profiling bounds");
  const report = { schema: "dimensify-worker-sampled-profiles/1", cohort: "worker-profile-diagnostic", exactWorkerThreadCpuMs: null, exactWorkerThreadCpuUnavailableReason: "V8 Profiler samples stack presence and elapsed sample deltas; no OS worker thread CPU clock is exposed by this harness", samplingIntervalUs, scope: "dedicated workers created by the page after attachment; all worker activity, including idle, GC and cancelled jobs; no decode-success attribution", perturbation: "workers pause before execution; parent Worker.terminate is delayed while every active worker profile is flushed and restarted; profiling and protocol overhead exclude this cohort from frame/loading performance publication", parentTerminationRequests: 0, parentTerminationFlushTimeouts: 0, workers: [], errors: [], sampleLimitReached: false, stopped: false };
  const session = await context.newCDPSession(page);
  const active = new Map(), pending = new Map(), attaching = new Set();
  let nextId = 0, sampleCount = 0, nodeCount = 0, stopping = false, chain = Promise.resolve();
  const send = (sessionId, method, params = {}) => new Promise((resolve, reject) => {
    const id = ++nextId;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Worker protocol command timed out: ${method}`)); }, commandTimeoutMs);
    pending.set(id, { resolve, reject, timer });
    session.send("Target.sendMessageToTarget", { sessionId, message: JSON.stringify({ id, method, params }) }).catch(error => { const item = pending.get(id); if (item) { clearTimeout(item.timer); pending.delete(id); reject(error); } });
  });
  session.on("Target.receivedMessageFromTarget", event => {
    const message = JSON.parse(event.message), item = pending.get(message.id);
    if (!item) return;
    clearTimeout(item.timer); pending.delete(message.id);
    message.error ? item.reject(new Error(message.error.message)) : item.resolve(message.result);
  });
  session.on("Target.detachedFromTarget", event => {
    const worker = active.get(event.sessionId);
    if (worker) { worker.detached = true; worker.unflushedTail = worker.profiling; active.delete(event.sessionId); }
  });
  const attach = async event => {
    if (event.targetInfo.type !== "worker") return;
    const worker = { targetId: event.targetInfo.targetId, url: event.targetInfo.url, segments: [], detached: false, profiling: false, unflushedTail: false, completionStatus: "unavailable; profile does not identify successful versus cancelled decode jobs" };
    if (report.workers.length >= maxWorkers || stopping) {
      report.errors.push("Worker attachment limit reached or profiling stopping; worker omitted");
      await send(event.sessionId, "Runtime.runIfWaitingForDebugger").catch(() => {});
      return;
    }
    report.workers.push(worker); active.set(event.sessionId, worker);
    try {
      await send(event.sessionId, "Profiler.enable");
      await send(event.sessionId, "Profiler.setSamplingInterval", { interval: samplingIntervalUs });
      await send(event.sessionId, "Profiler.start"); worker.profiling = true;
    } catch { worker.error = "Worker Profiler setup unavailable"; }
    finally { await send(event.sessionId, "Runtime.runIfWaitingForDebugger").catch(() => { worker.error = "Worker resume failed"; }); }
  };
  session.on("Target.attachedToTarget", event => {
    const task = attach(event); attaching.add(task);
    task.catch(() => report.errors.push("Worker attachment failed")).finally(() => attaching.delete(task));
  });
  const flush = async (reason, restart) => {
    await Promise.allSettled([...attaching]);
    for (const [sessionId, worker] of [...active]) {
      if (!worker.profiling) continue;
      try {
        const { profile } = await send(sessionId, "Profiler.stop"); worker.profiling = false;
        const summary = profileSummary(profile);
        if (sampleCount + summary.samples > maxSamples || nodeCount + profile.nodes.length > maxNodes) {
          report.sampleLimitReached = true; worker.omittedProfileReason = "Bounded sample/node retention limit exceeded";
        } else { sampleCount += summary.samples; nodeCount += profile.nodes.length; worker.segments.push({ reason, summary, profile }); }
        if (restart && !report.sampleLimitReached && !worker.detached) { await send(sessionId, "Profiler.start"); worker.profiling = true; }
      } catch { worker.unflushedTail = true; worker.error = "Worker profile unavailable (detached, terminated or protocol failure)"; }
    }
  };
  const enqueue = (reason, restart) => { const operation = chain.then(() => flush(reason, restart)); chain = operation.catch(() => {}); return operation; };
  try {
    await page.exposeBinding(BINDING, async (_source, detail) => {
      if (detail?.kind === "timeout") { report.parentTerminationFlushTimeouts++; return; }
      report.parentTerminationRequests++;
      if (!stopping) await enqueue("before-parent-terminate-request; worker/job association unavailable", true);
    });
    await page.addInitScript(({ binding, timeoutMs }) => {
      const original = Worker.prototype.terminate;
      Worker.prototype.terminate = function () {
        const worker = this;
        // Bounded best effort; a timeout becomes an unflushed tail, never a successful-job claim.
        let timer;
        Promise.race([globalThis[binding]({ kind: "flush" }), new Promise(resolve => { timer = setTimeout(() => { globalThis[binding]({ kind: "timeout" }).catch(() => {}); resolve(); }, timeoutMs); })]).catch(() => {}).finally(() => { clearTimeout(timer); original.call(worker); });
      };
    }, { binding: BINDING, timeoutMs: commandTimeoutMs * 2 });
    await session.send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: false, filter: [{ type: "worker", exclude: false }, { exclude: true }] });
  } catch (error) { await session.detach().catch(() => {}); throw new Error("Worker CPU sampled profiling unavailable: page CDP auto-attach/Profiler setup failed"); }
  return { snapshot: () => JSON.parse(JSON.stringify(report)), async stop() {
    if (!stopping) {
      stopping = true;
      await enqueue("end-of-diagnostic-cohort", false);
      await session.send("Target.setAutoAttach", { autoAttach: false, waitForDebuggerOnStart: false, flatten: false }).catch(() => {});
      await session.detach().catch(() => {});
      report.stopped = true;
    }
    return JSON.parse(JSON.stringify(report));
  } };
}
module.exports = { startWorkerProfiling, profileSummary };
