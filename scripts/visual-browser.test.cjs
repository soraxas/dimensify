const test = require("node:test");
test("privacy-redacted adapters require separate platform and explicit hardware profiles", () => {
  const { laneFor } = require("./visual-browser.cjs");
  const check = require("node:assert/strict");
  check.equal(laneFor("linux"), "browser-linux-swiftshader");
  check.throws(() => laneFor("darwin"), /profile/);
  check.throws(() => laneFor("win32"), /profile/);
  check.throws(() => laneFor("darwin", null, "bad profile"), /profile/);
  check.notEqual(laneFor("linux"), laneFor("darwin", null, "apple-m5-max"));
  check.notEqual(laneFor("darwin", null, "apple-m1"), laneFor("darwin", null, "apple-m5-max"));
  check.notEqual(laneFor("darwin", null, "apple-m5-max"), laneFor("darwin", { id: "phone-a" }));
  check.notEqual(laneFor("darwin", { id: "phone-a" }), laneFor("darwin", { id: "phone-b" }));
});
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const visual = require("./visual-browser.cjs");
function status() {
  return {schema:"dimensify-visual-status/1",status:"complete",case:"anisotropic",ready:true,rendered:true,width:256,height:256,
    metadata:{lane:"wasm",target:"wasm32",backend:"BrowserWebGpu",adapter:"test adapter",driver:"browser",browser:"none",viewport:{width:256,height:256},sceneHash:"a".repeat(64),settingsHash:"b".repeat(64)},
    rgba:[128,128,128,128,129,...Array(256*256*4-5).fill(128)],semanticChecks:{passed:true},captures:[{name:"anisotropic",file:"anisotropic.png",width:256,height:256,sortEncoded:true,drawEncoded:true,captureCompleted:true}]};
}
test("capture rejects incomplete readiness, missing adapter, empty pixels, bad dimensions and device failure", () => {
  assert.equal(visual.validateStatus(status(), "anisotropic").width,256);
  for(const mutate of [s=>s.ready=false,s=>s.rendered=false,s=>s.status="loading",s=>s.case="wrong",s=>s.metadata.adapter="",s=>s.metadata.backend="",s=>s.rgba=[],s=>s.rgba[0]=300,s=>s.width=255,s=>s.error="device lost",s=>s.semanticChecks.passed=false,s=>s.captures[0].sortEncoded=false,s=>s.captures[0].drawEncoded=false,s=>s.captures[0].captureCompleted=false,s=>s.captures=[],s=>s.rgba.fill(0),s=>s.rgba.fill(128),s=>s.metadata.sceneHash="invalid"]) {
    const input=status(); mutate(input); assert.throws(()=>visual.validateStatus(input,"anisotropic"));
  }
});
test("manifest enforces the complete inventory with one distinct correctly named PNG per case",()=>{
  const captures=visual.CASES.map(name=>({name,file:`${name}.png`,width:256,height:256}));
  assert.equal(visual.buildManifest(status().metadata,captures).schema,"dimensify-visual-captures/1");
  for(const bad of [[],captures.slice(1),[...captures,captures[0]],captures.map((x,i)=>i?x:{...x,file:"../outside.png"}),captures.map((x,i)=>i?x:{...x,width:1})]) assert.throws(()=>visual.buildManifest(status().metadata,bad));
});
test("new output refuses existing directories and saves independently decoded RGBA images",()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),"visual-browser-test-"));
  try {
    assert.throws(()=>visual.prepareOutput(root));
    const out=visual.prepareOutput(path.join(root,"fresh"));
    visual.writeCapture(out,status(),"anisotropic");
    const {PNG}=require("pngjs"); const png=PNG.sync.read(fs.readFileSync(path.join(out,"anisotropic.png")));
    assert.equal(png.width,256); assert.equal(png.height,256); assert.deepEqual([...png.data.subarray(0,4)],[128,128,128,128]);
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});
test("physical mobile identity requires explicit device evidence and a remote connection",()=>{
  const target={kind:"physical-mobile",id:"phone-01",model:"Pixel 9",os:"Android 16",provider:"local-usb"};
  assert.deepEqual(visual.validateTarget(target,true),target);
  assert.throws(()=>visual.validateTarget(target,false));
  for(const bad of [{kind:"emulated-mobile"},{...target,model:""},{...target,id:""},{...target,os:""},{...target,provider:""}]) assert.throws(()=>visual.validateTarget(bad,true));
});
test("diagnostics strip endpoint credentials and signed query values",()=>{
  const endpoint="wss://user:password@example.test/playwright?token=topsecret";
  const safe=visual.sanitize(`Failed ${endpoint} and https://host.test/a?signature=secret#private`,[endpoint]);
  for(const value of ["password","topsecret","signature=secret","#private"]) assert.equal(safe.includes(value),false);
  assert.throws(()=>visual.validateUrl("https://user:password@host.test"));
  assert.throws(()=>visual.validateUrl("file:///etc/passwd"));
});

test("operator mobile identity is cross-checked against the connected browser without inferring physical status",()=>{
  const target={kind:"physical-mobile",id:"phone-01",model:"Pixel 9",os:"Android 16",provider:"local-usb"};
  assert.doesNotThrow(()=>visual.validateObservedTarget(target,{userAgent:"Mozilla/5.0 (Linux; Android 16)",platform:"Linux",maxTouchPoints:5}));
  assert.throws(()=>visual.validateObservedTarget(target,{userAgent:"Mozilla/5.0 (Macintosh)",platform:"MacIntel",maxTouchPoints:0}));
  assert.throws(()=>visual.validateTarget({...target,os:"Windows 11"},true));
});
