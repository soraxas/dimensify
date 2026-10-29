const test = require('node:test');
const assert = require('node:assert/strict');
const { createGpuCollector } = require('./perf-browser-gpu.cjs');
const sample = (selection, duration=1) => ({identity:{selection:String(selection),frame:String(selection),view:'1',scene:'2'},measurement:{duration_ms:duration,failure:null}});
const snapshot = (completed,samples,frame='20') => ({gpu:{passes:[{scope:'raster',completed,samples,capability:'timestamp_query'}],radix:null,attribution:{current_frame:frame}}});
test('delayed GPU arrival follows originating frame, with distinct segments retained',()=>{
  const c=createGpuCollector();
  c.observe(snapshot(2,[sample(10),sample(11)]));
  c.observe(snapshot(3,[sample(10),sample(11),sample(11,2)]));
  assert.equal(c.between('9','10').length,1);
  assert.equal(c.between('10','11').length,2);
  assert.equal(c.between('11','20').length,0);
});
test('evicted history gaps, resets and retention bounds cannot silently pass',()=>{
  const c=createGpuCollector({maxSamples:3});
  c.observe(snapshot(1,[sample(1)]));
  c.observe(snapshot(4,[sample(4)]));
  assert.equal(c.report().historyGaps,2);
  assert.throws(()=>c.observe(snapshot(3,[sample(3)])),/reset/);
  c.observe(snapshot(5,[sample(5)]));
  assert.throws(()=>c.observe(snapshot(6,[sample(6)])),/limit/);
});
test('64-bit source IDs survive JSON and missing capabilities stay unavailable',()=>{
  const c=createGpuCollector();
  c.observe({gpu:null});
  c.observe(snapshot(1,[sample('18446744073709551614')], '18446744073709551614'));
  assert.equal(c.between('18446744073709551613','18446744073709551614').length,1);
  assert.throws(()=>c.between(9007199254740992,'18446744073709551614'),/identity/);
});
