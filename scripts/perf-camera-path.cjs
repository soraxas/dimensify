// Timed-path correctness and cadence diagnostics; never uploads to Bencher.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { parseArgs } = require("node:util");
const { prepareOutput } = require("./perf-browser-output.cjs");
const { summarize } = require("./perf-browser-results.cjs");
function expectedOrbit(start, elapsedMs, durationMs = 6000) {
  const t = Math.max(0, Math.min(1, elapsedMs / durationMs));
  const u = t <= .5 ? t * 2 : (1 - t) * 2;
  const ease = u * u * u * (u * (u * 6 - 15) + 10);
  const angle = .7 * ease, c = Math.cos(angle), s = Math.sin(angle);
  const [x,y,z,w] = start.rotation_xyzw;
  // Forward is quaternion * -Z; this independently reconstructs the tour pivot.
  const forward = [-2*(x*z+w*y), -2*(y*z-w*x), -(1-2*(x*x+y*y))];
  const pivot = start.position.map((v,i) => v + forward[i] * start.distance);
  const [dx,dy,dz] = start.position.map((v,i) => v - pivot[i]);
  const sh = Math.sin(angle/2), ch = Math.cos(angle/2);
  return { position: [pivot[0]+c*dx+s*dz, pivot[1]+dy, pivot[2]-s*dx+c*dz],
    rotation_xyzw: [ch*x+sh*z, ch*y+sh*w, ch*z-sh*x, ch*w-sh*y], fov_radians: start.fov_radians };
}
function assertPose(actual, expected) {
  if (![...actual.position, ...actual.rotation_xyzw, actual.fov_radians].every(Number.isFinite)) throw Error("Timed camera path diverged: nonfinite pose");
  const positionError = Math.hypot(...actual.position.map((v,i) => v-expected.position[i]));
  const dot = Math.abs(actual.rotation_xyzw.reduce((a,v,i) => a+v*expected.rotation_xyzw[i],0) / (Math.hypot(...actual.rotation_xyzw) * Math.hypot(...expected.rotation_xyzw)));
  const angle = 2*Math.acos(Math.min(1,dot));
  const positionTolerance = Math.max(1e-4, 8 * 2**-23 * Math.max(...actual.position.map(Math.abs), ...expected.position.map(Math.abs)));
  if (!Number.isFinite(positionError) || positionError > positionTolerance || !Number.isFinite(angle) || angle > 1e-4 || Math.abs(actual.fov_radians-expected.fov_radians) > 1e-6) throw Error("Timed camera path diverged from analytic pose");
  return positionError;
}
function validateSamples(record) {
  const samples = record.samples;
  if (samples.length < 10 || !samples.some(v => v.camera_path.elapsed_ms > 0 && v.camera_path.elapsed_ms < 2000) || !samples.some(v => v.camera_path.elapsed_ms > 4000 && v.camera_path.elapsed_ms < 6000)) throw Error("Insufficient intermediate camera observations");
  let previous = -1;
  const start = {...record.before.camera, distance:samples[0].camera_path.distance};
  if (!Number.isFinite(start.distance) || start.distance <= 0) throw Error("Missing camera orbit distance");
  const errors = samples.map(v => {
    const p = v.camera_path;
    if (p.protocol !== "absolute-quintic-v1" || p.generation !== record.before.camera_path.generation+1 || p.duration_ms !== 6000 || !Number.isFinite(p.elapsed_ms) || p.elapsed_ms <= previous || p.elapsed_ms > 6000 || p.elapsed_ms-previous > 1000) throw Error("Incomplete or inconsistent camera path observations");
    previous = p.elapsed_ms;
    return assertPose(v.camera,expectedOrbit(start,p.elapsed_ms));
  });
  if (samples.at(-1).camera_path.active || previous !== 6000) throw Error("Camera path did not reach its endpoint");
  assertPose(samples.at(-1).camera,record.before.camera);
  return Math.max(...errors);
}
function isQuiescent(value) {
  return value?.phase === "ready" && value.render_ready_splats > 0 && value.renderer_encoded_frames > 0
    && value.streaming_busy === false && value.pending_downloads === 0 && value.pending_generations === 0
    && !value.camera_path?.active && !value.notice
    && (value.stream_statuses || []).every(s => !s.pending_work && !s.pending_generation && !(s.pending_downloads > 0));
}
function assertCancellation(before, after) {
  if(after.camera_path.active) throw Error("Pointer did not cancel camera tour");
  assertPose(after.camera, before.camera);
}
async function waitForQuiescence(page) {
  const deadline = Date.now() + 180000;
  let since = null, signature = null;
  while(Date.now() < deadline) {
    const value = await page.evaluate(() => {
      const t=window.__DIMENSIFY_PERF__;
      return t && {phase:t.phase,render_ready_splats:t.render_ready_splats,renderer_encoded_frames:t.renderer_encoded_frames,
        streaming_busy:t.streaming_busy,pending_downloads:t.pending_downloads,pending_generations:t.pending_generations,
        camera_path:{active:t.camera_path?.active},notice:t.notice,stream_statuses:t.stream_statuses};
    });
    const current = JSON.stringify(value?.stream_statuses) + ":" + value?.render_ready_splats;
    if(!isQuiescent(value) || current !== signature) since = null;
    if(isQuiescent(value)) {
      since ??= Date.now();
      if(Date.now() - since >= 1000) return {stable_ms:Date.now()-since, ...value};
    }
    signature=current;
    await page.waitForTimeout(100);
  }
  throw Error("Scene did not reach stable streaming quiescence before timing");
}
async function main() {
  const {values: args} = parseArgs({options:{ url:{type:"string"}, file:{type:"string"}, out:{type:"string"}, rounds:{type:"string",default:"3"}, headed:{type:"boolean",default:false}, capture:{type:"boolean",default:false} }});
  const rounds = Number(args.rounds);
  if (!args.url || !args.file || !args.out || !Number.isInteger(rounds) || rounds < 1 || rounds > 10) throw Error("Use --url URL --file scene.ply --out NEW_DIRECTORY [--rounds 3] [--headed] [--capture]");
  const output = prepareOutput(args.out), file = path.resolve(args.file), url = new URL(args.url);
  url.searchParams.set("perf", "1");
  const fixture = {name:path.basename(file),sha256:crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex")};
  const {chromium} = require(process.env.PLAYWRIGHT_MODULE || "playwright");
  const browser = await chromium.launch({headless:!args.headed, executablePath:process.env.CHROMIUM_EXECUTABLE,
    args:["--enable-unsafe-webgpu", ...(process.platform === "darwin" ? ["--use-angle=metal"] : ["--enable-features=Vulkan", "--disable-vulkan-surface"])]});
  const raw = {schema:"dimensify-camera-path-diagnostic/1", protocol:"absolute-quintic-v1", performancePublicationEligible:false,
    rotationToleranceRadians:1e-4, fovToleranceRadians:1e-6,
    positionTolerance:"max(1e-4 world units, 8 f32 epsilon times maximum coordinate magnitude)",
    scope:"same-session warmed camera tour; app/encoded cadence, not GPU completion or display FPS; no baseline speedup claim",
    fixture,
    captureMode:args.capture,
    artifactEvidence:"SHA256 fetched from observed WASM URLs after measurement; not browser execution attestation",
    wasmResponses:[],
    browser:browser.version(), platform:process.platform, arch:process.arch, viewport:{width:1000,height:760}, artifacts:{}, rounds:[], errors:[]};
  let page;
  try {
    try {
      const response = await fetch(new URL("build.json", url));
      if(response.ok) raw.build = await response.json();
      else raw.buildProvenanceUnavailable = "Served build.json unavailable; served WASM hashes retained";
    } catch { raw.buildProvenanceUnavailable = "Served build.json unavailable; served WASM hashes retained"; }
    page = await browser.newPage({viewport:raw.viewport});
    page.on("pageerror", e => raw.errors.push(String(e)));
    page.on("console", m => {if(m.type()==="error") raw.errors.push(m.text());});
    page.on("response", response => {
      if(response.url().split("?")[0].endsWith(".wasm")) raw.wasmResponses.push({url:response.url(),status:response.status()});
    });
    await page.goto(url.href);
    await page.locator("#dimensify-file-input").waitFor({state:"attached",timeout:180000});
    await page.locator("#dimensify-file-input").setInputFiles(file);
    await page.waitForFunction(() => window.__DIMENSIFY_PERF__?.phase === "ready" && window.__DIMENSIFY_PERF__.renderer_encoded_frames > 0, null, {timeout:180000});
    await page.mouse.click(600,400);
    await page.keyboard.press("f");
    raw.warmup = await waitForQuiescence(page);
    raw.environment = await page.evaluate(() => ({dpr:devicePixelRatio, canvas:{width:document.querySelector("canvas").width,height:document.querySelector("canvas").height},userAgent:navigator.userAgent}));
    raw.initial = await page.evaluate(() => window.__DIMENSIFY_PERF__);
    if(raw.initial.camera_path?.protocol !== raw.protocol) throw Error("Build lacks timed camera path telemetry");
    for(let round=1;round<=rounds;round++) {
      const settled = await waitForQuiescence(page);
      const before = await page.evaluate(() => window.__DIMENSIFY_PERF__);
      const generation = before.camera_path.generation;
      await page.keyboard.press("p");
      await page.waitForFunction(g => window.__DIMENSIFY_PERF__?.camera_path?.generation === g+1 && window.__DIMENSIFY_PERF__.camera_path.active, generation, {timeout:10000});
      const record = {round, before, settled, samples:[]}; raw.rounds.push(record);
      let last = -1, middle = false;
      const deadline = Date.now()+30000;
      while(true) {
        if(raw.errors.length) throw Error(raw.errors.join("\n"));
        // Poll only pose provenance. Repeatedly transferring GPU histories over
        // CDP can hide observations and distort the workload being measured.
        const value = await page.evaluate(() => {
          const t=window.__DIMENSIFY_PERF__;
          return {observed_at_ms:t.observed_at_ms,camera:t.camera,camera_path:t.camera_path};
        });
        if(value.observed_at_ms !== last) { record.samples.push(value); last=value.observed_at_ms; }
        if(args.capture && !middle && value.camera_path.elapsed_ms >= 3000 && value.camera_path.elapsed_ms < 4000) { await page.screenshot({path:path.join(output,`round-${round}-middle.png`)}); middle=true; }
        if(!value.camera_path.active && value.camera_path.elapsed_ms === 6000) break;
        if(Date.now()>deadline) throw Error("Camera tour did not complete");
        await page.waitForTimeout(30);
      }
      if(args.capture && !middle) throw Error("No intermediate image captured");
      record.maxPositionError = validateSamples(record);
      record.final = await page.evaluate(() => window.__DIMENSIFY_PERF__);
      const first = record.samples[0].observed_at_ms, end = record.samples.at(-1).observed_at_ms, final = record.final;
      const intervals = history => history.intervals.filter(v => v[1]>=first && v[1]<=end).map(v=>v[2]);
      record.appIntervalMs = args.capture ? null : summarize(intervals(final.app_frames));
      record.rendererEncodedIntervalMs = args.capture ? null : summarize(intervals(final.renderer_frames));
      await page.screenshot({path:path.join(output,`round-${round}-end.png`)});
    }
    const interruptionGeneration = await page.evaluate(() => window.__DIMENSIFY_PERF__.camera_path.generation);
    await page.keyboard.press("p");
    await page.waitForFunction(g => window.__DIMENSIFY_PERF__.camera_path.generation === g+1 && window.__DIMENSIFY_PERF__.camera_path.active, interruptionGeneration, {timeout:10000});
    await page.waitForTimeout(1000);
    raw.interruptionStart = await page.evaluate(() => ({camera_path:window.__DIMENSIFY_PERF__.camera_path,camera:window.__DIMENSIFY_PERF__.camera}));
    if(!raw.interruptionStart.camera_path.active) throw Error("Tour stopped before interruption test");
    await page.evaluate(() => window.addEventListener("pointerdown", () => {
      const t=window.__DIMENSIFY_PERF__;
      window.__DIMENSIFY_INTERRUPTION_START__={camera:t.camera,camera_path:t.camera_path};
    }, {once:true}));
    await page.mouse.move(600,400); await page.mouse.down();
    raw.interruptionStart = await page.evaluate(() => window.__DIMENSIFY_INTERRUPTION_START__);
    await page.waitForTimeout(200); await page.mouse.up();
    raw.interrupted = await page.evaluate(() => ({camera_path:window.__DIMENSIFY_PERF__.camera_path,camera:window.__DIMENSIFY_PERF__.camera}));
    assertCancellation(raw.interruptionStart, raw.interrupted);
    if(raw.errors.length) throw Error(raw.errors.join("\n"));
    raw.result="PASS";
    const summary = {result:raw.result,output,rounds:raw.rounds.map(r=>({round:r.round,samples:r.samples.length,maxPositionError:r.maxPositionError,appIntervalMs:r.appIntervalMs,rendererEncodedIntervalMs:r.rendererEncodedIntervalMs})),performancePublicationEligible:false,scope:raw.scope};
    fs.writeFileSync(path.join(output,"summary.json"),JSON.stringify(summary,null,2));
    console.log(JSON.stringify(summary));
  } catch(error) {
    raw.result="FAIL"; raw.failure=String(error);
    if(page) await page.screenshot({path:path.join(output,"failure.png")}).catch(()=>{});
    throw error;
  } finally {
    // Hash after measurement: retrieving a large WASM response over CDP can
    // interfere with observation delivery and has no place in timed tours.
    for(const resource of raw.wasmResponses) {
      try {
        const response = await fetch(resource.url);
        if(!response.ok) throw Error(`HTTP ${response.status}`);
        raw.artifacts[new URL(resource.url).pathname] = crypto.createHash("sha256").update(Buffer.from(await response.arrayBuffer())).digest("hex");
      } catch(error) { resource.hashUnavailable=String(error); }
    }
    fs.writeFileSync(path.join(output,"raw.json"),JSON.stringify(raw,null,2));
    await browser.close();
  }
}
if(require.main===module) main().catch(e=>{console.error(e);process.exitCode=1;});
module.exports={expectedOrbit,assertPose,validateSamples,isQuiescent,assertCancellation};
