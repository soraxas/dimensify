const {test} = require("node:test");
const assert = require("node:assert/strict");
const {expectedOrbit,assertPose,validateSamples,isQuiescent,assertCancellation} = require("./perf-camera-path.cjs");
const start = {position:[0,0,4],rotation_xyzw:[0,0,0,1],distance:4,fov_radians:.8};
test("analytic tour returns exactly and keeps its pivot and lens",()=>{
  assertPose(expectedOrbit(start,0),start);
  assertPose(expectedOrbit(start,6000),start);
  const middle=expectedOrbit(start,3000);
  assert.ok(Math.abs(middle.position[0]-4*Math.sin(.7))<1e-12);
  assert.ok(Math.abs(middle.position[2]-4*Math.cos(.7))<1e-12);
  assert.equal(middle.fov_radians,.8);
});
test("analytic pose checks reject intermediate drift and malformed samples",()=>{
  const expected=expectedOrbit(start,1500);
  assert.throws(()=>assertPose({...expected,position:[0,0,4]},expected),/diverged/);
  assert.throws(()=>assertPose({...expected,position:[NaN,0,4]},expected),/diverged/);
});
test("analytic tour is symmetric and eases from rest",()=>{
  assertPose(expectedOrbit(start,731),expectedOrbit(start,6000-731));
  assert.ok(expectedOrbit(start,1).position[0]<1e-6);
});

test("terminal-only and incomplete sampled paths cannot pass",()=>{
  const before={camera:start,camera_path:{generation:0}};
  const sample=ms=>({camera:expectedOrbit(start,ms),camera_path:{protocol:"absolute-quintic-v1",generation:1,elapsed_ms:ms,duration_ms:6000,distance:4,active:ms<6000}});
  assert.throws(()=>validateSamples({before,samples:[sample(6000)]}),/intermediate/);
  const samples=Array.from({length:60},(_,i)=>sample((i+1)*100));
  assert.equal(validateSamples({before,samples}),0);
  samples[15].camera_path.generation=2;
  assert.throws(()=>validateSamples({before,samples}),/inconsistent/);
});

test("orientation checks reject small rotation drift and allow quaternion sign",()=>{
  assertPose({...start,rotation_xyzw:[0,0,0,-1]},start);
  assert.throws(()=>assertPose({...start,rotation_xyzw:[0,Math.sin(.001/2),0,Math.cos(.001/2)]},start),/diverged/);
});

test("path observations reject long gaps, wrong duration and changed lens",()=>{
  const before={camera:start,camera_path:{generation:0}};
  const samples=()=>Array.from({length:60},(_,i)=>{const ms=(i+1)*100;return {camera:expectedOrbit(start,ms),camera_path:{protocol:"absolute-quintic-v1",generation:1,elapsed_ms:ms,duration_ms:6000,distance:4,active:ms<6000}};});
  const gap=samples().filter(v=>v.camera_path.elapsed_ms<2000 || v.camera_path.elapsed_ms>3100);
  assert.throws(()=>validateSamples({before,samples:gap}),/inconsistent/);
  const duration=samples(); duration[20].camera_path.duration_ms=5000;
  assert.throws(()=>validateSamples({before,samples:duration}),/inconsistent/);
  const lens=samples(); lens[20].camera.fov_radians+=0.1;
  assert.throws(()=>validateSamples({before,samples:lens}),/diverged/);
});
test("warmed tours require evaluated renderer and zero pending work",()=>{
 const ready={phase:"ready",render_ready_splats:8,renderer_encoded_frames:1,streaming_busy:false,pending_downloads:0,pending_generations:0,stream_statuses:[]};
 assert.equal(isQuiescent(ready),true);
 for(const patch of [{phase:"loading"},{render_ready_splats:0},{renderer_encoded_frames:0},{notice:"stale"},{stream_statuses:[{pending_generation:true}]},{streaming_busy:true},{pending_downloads:1},{pending_generations:1},{stream_statuses:[{pending_work:true}]},{camera_path:{active:true}}]) assert.equal(isQuiescent({...ready,...patch}),false);
});
test("interruption must preserve the visible pose",()=>{
 const before={camera:start};
 assert.doesNotThrow(()=>assertCancellation(before,{camera:start,camera_path:{active:false}}));
 assert.throws(()=>assertCancellation(before,{camera:{...start,position:[1,2,3]},camera_path:{active:false}}),/diverged/);
});
