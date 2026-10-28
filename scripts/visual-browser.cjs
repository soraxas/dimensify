#!/usr/bin/env node
// Actual renderer readbacks only. This runner does not measure performance.
const fs = require("node:fs");
const path = require("node:path");
const {parseArgs} = require("node:util");
const CASES = Object.freeze(require("./visual-browser-cases.json"));
const SIZE = 256;
const IDENTITY = ["lane", "target", "backend", "adapter", "driver", "browser", "viewport", "sceneHash", "settingsHash"];

function validateUrl(value) {
  const url = new URL(value);
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error("Testbed URL must be HTTP(S) without credentials");
  return url;
}
function sanitize(value, secrets = []) {
  let result = String(value);
  for (const secret of secrets.filter(Boolean).sort((a,b)=>b.length-a.length)) result = result.split(secret).join("[remote endpoint redacted]");
  return result.replace(/(?:https?|wss?):\/\/[^\s<>"']+/g, (match) => {
    try { const url = new URL(match); url.username=""; url.password=""; if(url.search) url.search="?[redacted]"; url.hash=""; return url.href; } catch { return "[URL redacted]"; }
  });
}
function validateMetadata(metadata) {
  if (!metadata || typeof metadata !== "object") throw new Error("Missing renderer metadata");
  for (const key of IDENTITY.filter(key=>key!=="viewport")) if (typeof metadata[key] !== "string" || !metadata[key].trim()) throw new Error(`Missing renderer metadata: ${key}`);
  for (const key of ["sceneHash", "settingsHash"]) if (!/^[a-f0-9]{64}$/.test(metadata[key])) throw new Error(`Invalid ${key}`);
  if (metadata.viewport?.width !== SIZE || metadata.viewport?.height !== SIZE) throw new Error("Renderer viewport must be 256x256");
  return metadata;
}
function validateStatus(status, name) {
  if (!status || status.schema !== "dimensify-visual-status/1" || status.status !== "complete" || status.ready !== true || status.rendered !== true || status.case !== name) throw new Error(`Incomplete renderer capture for ${name}`);
  if (status.error || status.errors?.length || status.semanticChecks?.passed !== true) throw new Error(`Renderer semantic/device failure for ${name}`);
  if(status.width !== SIZE || status.height !== SIZE) throw new Error(`Invalid render dimensions for ${name}`);
  validateMetadata(status.metadata);
  const capture = status.captures?.[0];
  if(status.captures?.length!==1 || capture?.name!==name || capture.file!==`${name}.png` || capture.width!==SIZE || capture.height!==SIZE || capture.sortEncoded!==true || capture.drawEncoded!==true || capture.captureCompleted!==true) throw new Error(`Missing completed renderer sort/draw/readback acknowledgement for ${name}`);
  if(!Array.isArray(status.rgba) || status.rgba.length !== SIZE*SIZE*4 || status.rgba.some(x=>!Number.isInteger(x)||x<0||x>255)) throw new Error(`Missing or invalid renderer RGBA for ${name}`);
  let visible=false, varied=false;
  for(let i=0;i<status.rgba.length;i+=4) { if(status.rgba[i+3]) visible=true; if(status.rgba[i]!==status.rgba[0] || status.rgba[i+1]!==status.rgba[1] || status.rgba[i+2]!==status.rgba[2] || status.rgba[i+3]!==status.rgba[3]) varied=true; }
  if(!visible || !varied) throw new Error(`Blank renderer readback for ${name}`);
  return status;
}
function prepareOutput(directory) {
  const output=path.resolve(directory);
  fs.mkdirSync(output,{recursive:false});
  return output;
}
function validateTarget(target, remote) {
  if (!remote || target?.kind !== "physical-mobile") throw new Error("Physical mobile capture requires explicit identity and a remote browser connection");
  for(const key of ["id","model","os","provider"]) if(typeof target[key]!=="string" || !target[key].trim() || target[key].length>200 || /[\r\n]/.test(target[key])) throw new Error(`Missing or invalid physical device ${key}`);
  if(!/android|ios|ipados/i.test(target.os)) throw new Error("Physical mobile OS must identify Android, iOS or iPadOS");
  return Object.fromEntries(["kind","id","model","os","provider"].map(key=>[key,target[key]]));
}
function validateObservedTarget(target, observed) {
  if(!target) return;
  const compatible=/android/i.test(target.os) ? /android/i.test(observed.userAgent) : /iphone|ipad|ipod/i.test(observed.userAgent) || (observed.platform==="MacIntel" && observed.maxTouchPoints>1);
  if(!compatible) throw new Error("Declared physical mobile OS does not match the connected browser; desktop emulation cannot establish physical-device identity");
}
function writeCapture(output,status,name) {
  validateStatus(status,name);
  const {PNG}=require("pngjs");
  const png=new PNG({width:SIZE,height:SIZE}); png.data=Buffer.from(status.rgba);
  const file=`${name}.png`;
  fs.writeFileSync(path.join(output,file),PNG.sync.write(png),{flag:"wx"});
  const decoded=PNG.sync.read(fs.readFileSync(path.join(output,file)));
  if(decoded.width!==SIZE || decoded.height!==SIZE || !decoded.data.equals(png.data)) throw new Error(`Capture roundtrip failed: ${name}`);
  return {...status.captures[0],name,file,width:SIZE,height:SIZE,semanticChecks:status.captures[0].semanticChecks || status.semanticChecks};
}
function buildManifest(metadata,captures) {
  validateMetadata(metadata);
  if(!Array.isArray(captures) || captures.length!==CASES.length || CASES.some((name,i)=>captures[i]?.name!==name || captures[i]?.file!==`${name}.png` || captures[i]?.width!==SIZE || captures[i]?.height!==SIZE)) throw new Error("Capture inventory must match every committed case exactly");
  return {schema:"dimensify-visual-captures/1",metadata,captures};
}
function softwareFlags(platform=process.platform) {
  if(platform === "linux") return ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=vulkan", "--use-vulkan=swiftshader", "--use-webgpu-adapter=swiftshader", "--disable-vulkan-surface"];
  return ["--enable-unsafe-webgpu", ...(platform === "darwin" ? ["--use-angle=metal"] : [])];
}
function laneFor(platform=process.platform,target=null,profile=null) {
  if(target) return `browser-mobile-${target.id}`;
  const requested=platform==="linux"?"swiftshader":platform==="darwin"?"metal":"default";
  if(platform==="linux") return `browser-${platform}-${requested}`;
  if(typeof profile!=="string" || !/^[a-z0-9][a-z0-9-]{0,99}$/.test(profile)) throw new Error("Local hardware capture requires --profile with a stable device/driver profile ID (lowercase letters, digits, hyphens)");
  return `browser-${platform}-${requested}-${profile}`;
}
async function capture(args, environment=process.env) {
  if(!args.url || !args.out) throw new Error("Usage: node scripts/visual-browser.cjs --url HTTP_TESTBED --out NEWDIR [--timeout-ms 180000] [--profile HARDWARE_ID] [--cdp-endpoint URL | --ws-endpoint URL] [--target device.json]");
  const base=validateUrl(args.url);
  const timeout=Number(args["timeout-ms"] || 180000);
  if(!Number.isInteger(timeout)||timeout<1000||timeout>600000) throw new Error("timeout-ms must be 1000..600000");
  const cdp=args["cdp-endpoint"] || environment.DIMENSIFY_CDP_ENDPOINT;
  const ws=args["ws-endpoint"] || environment.BROWSERSTACK_WS_ENDPOINT;
  if(cdp && ws) throw new Error("Select exactly one remote connection mechanism");
  const target=args.target ? validateTarget(JSON.parse(fs.readFileSync(args.target,"utf8")),Boolean(cdp||ws)) : null;
  if((cdp||ws) && !target) throw new Error("Remote capture requires --target physical-device identity JSON");
  const lane=laneFor(process.platform,target,args.profile);
  const output=prepareOutput(args.out);
  const secrets=[cdp,ws]; const logs=[]; const errors=[]; const captures=[];
  let gpuInfo={unavailable:"browser protocol did not provide GPU inventory"};
  const diagnostic={schema:"dimensify-visual-browser-diagnostics/1",startedAt:new Date().toISOString(),cases:[],errors:[],connection:cdp?"cdp":ws?"playwright-remote":"local",physicalDevice:target};
  let browser,context,page,metadata;
  const record=(message)=>{if(logs.length<5000)logs.push(sanitize(message,secrets));};
  try {
    const {chromium}=require(environment.PLAYWRIGHT_MODULE || "playwright");
    browser=cdp?await chromium.connectOverCDP(cdp,{timeout}):ws?await chromium.connect(ws,{timeout}):await chromium.launch({headless:!args.headed,executablePath:environment.CHROMIUM_EXECUTABLE,args:softwareFlags(),timeout});
    if(cdp) { context=browser.contexts()[0]; if(!context) throw new Error("Remote browser has no device context"); }
    else context=await browser.newContext(target?{}:{viewport:{width:SIZE,height:SIZE},deviceScaleFactor:1});
    try {
      const session=await browser.newBrowserCDPSession();
      try {gpuInfo=(await session.send("SystemInfo.getInfo")).gpu;} finally {await session.detach();}
    } catch {record("Browser GPU process inventory unavailable; renderer adapter metadata remains required");}
    page=await context.newPage(); page.setDefaultTimeout(timeout); page.setDefaultNavigationTimeout(timeout);
    page.on("pageerror",error=>{if(errors.length<500)errors.push(sanitize(error,secrets));record(error);});
    page.on("console",message=>{record(message.text());if((message.type()==="error" || message.text().includes("%cERROR%c")) && errors.length<500)errors.push(sanitize(message.text(),secrets));});
    page.on("crash",()=>errors.push("Browser page crashed"));
    for(const name of CASES) {
      const url=new URL(base);url.searchParams.set("case",name);
      const response=await page.goto(url.href,{waitUntil:"domcontentloaded"});
      if(!response || !response.ok()) throw new Error(`Testbed HTTP failure for ${name}`);
      await page.waitForFunction(()=>["complete","failed"].includes(window.__DIMENSIFY_VISUAL__?.status),null,{timeout});
      const status=await page.evaluate(()=>window.__DIMENSIFY_VISUAL__);
      diagnostic.cases.push({name,status:{...status,rgba:status?.rgba?`[${status.rgba.length} bytes]`:null}});
      if(errors.length) throw new Error(`Browser reported errors during ${name}`);
      validateStatus(status,name);
      const observed=await page.evaluate(()=>({userAgent:navigator.userAgent,platform:navigator.platform,maxTouchPoints:navigator.maxTouchPoints,dpr:devicePixelRatio,screen:{width:screen.width,height:screen.height,availWidth:screen.availWidth,availHeight:screen.availHeight},inner:{width:innerWidth,height:innerHeight},webgpuAvailable:Boolean(navigator.gpu)}));
      validateObservedTarget(target, observed);
      if(!observed.webgpuAvailable) throw new Error("WebGPU unavailable in captured browser");
      if(!Number.isFinite(observed.dpr)||observed.dpr<=0||observed.screen.width<=0||observed.screen.height<=0) throw new Error("Missing observed browser resolution/DPR");
      const current={...status.metadata,lane,target:target?target.id:status.metadata.target,browser:browser.version(),browserEnvironment:observed,physicalDevice:target,physicalDeviceEvidence:target?"explicit operator declaration plus observed OS/screen/DPR; browser identity alone does not attest physical hardware":null,captureSource:"Bevy ScreenshotCaptured RGBA",connection:diagnostic.connection,browserGpuInfo:gpuInfo,adapterEvidence:"renderer-selected adapter; browser GPU process inventory is supplemental and does not attest the WebGPU device"};
      if(metadata && IDENTITY.some(key=>JSON.stringify(metadata[key])!==JSON.stringify(current[key]))) throw new Error("Renderer identity changed between captures");
      metadata=current; diagnostic.metadata=metadata;
      captures.push(writeCapture(output,status,name));
    }
    const manifest=buildManifest(metadata,captures);
    if(errors.length) throw new Error("Browser errors after capture");
    fs.writeFileSync(path.join(output,"captures.json"),JSON.stringify(manifest,null,2)+"\n",{flag:"wx"});
    return {result:"PASS",output,captures:captures.length,metadata};
  } catch(error) {
    if(page) {
      let timer;
      try {
        diagnostic.lastStatus=await Promise.race([
          page.evaluate(()=>{const status=window.__DIMENSIFY_VISUAL__;return status?{...status,rgba:status.rgba?`[${status.rgba.length} bytes]`:null}:null;}),
          new Promise(resolve=>{timer=setTimeout(()=>resolve({unavailable:"page inspection timeout"}),2000);}),
        ]);
      } catch {diagnostic.lastStatus={unavailable:"page inspection failed"};}
      finally {clearTimeout(timer);}
    }
    diagnostic.errors.push(sanitize(error,secrets));
    throw new Error(sanitize(error,secrets));
  } finally {
    diagnostic.errors.push(...errors);
    fs.writeFileSync(path.join(output,"browser.log"),logs.join("\n")+"\n");
    fs.writeFileSync(path.join(output,"diagnostics.json"),sanitize(JSON.stringify(diagnostic,null,2),secrets)+"\n");
    if(page)await page.close().catch(()=>{});
    if(context && !cdp)await context.close().catch(()=>{});
    if(browser)await browser.close().catch(()=>{});
  }
}
async function main(mobile=false) {
  const {values:args}=parseArgs({options:{url:{type:"string"},out:{type:"string"},"timeout-ms":{type:"string"},profile:{type:"string"},"cdp-endpoint":{type:"string"},"ws-endpoint":{type:"string"},target:{type:"string"},headed:{type:"boolean"}}});
  if(mobile && !args.target)throw new Error("Physical mobile capture requires --target device.json and a remote connection");
  console.log(JSON.stringify(await capture(args)));
}
module.exports={CASES,validateUrl,sanitize,validateMetadata,validateStatus,prepareOutput,validateTarget,validateObservedTarget,writeCapture,buildManifest,softwareFlags,laneFor,capture,main};
if(require.main===module)main().catch(error=>{console.error(sanitize(error));process.exitCode=1;});
