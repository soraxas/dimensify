// Readback arrival is later than execution: keep the originating source frame.
function identity(value) {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) throw Error('Invalid GPU identity');
  if (!/^[0-9]+$/.test(String(value))) throw Error('Invalid GPU identity');
  return BigInt(value);
}
function createGpuCollector({maxSamples=200000}={}) {
  const completed=new Map(), samples=[], reports=new Map();
  let historyGaps=0;
  return {
    observe(value) {
      for (const report of [...(value?.gpu?.passes || []), ...(value?.gpu?.radix ? [value.gpu.radix] : [])]) {
        if (!report || typeof report.scope!=='string' || !Number.isSafeInteger(report.completed) || report.completed<0 || !Array.isArray(report.samples) || report.samples.length>report.completed) throw Error('Invalid GPU history');
        const last=completed.get(report.scope), first=report.completed-report.samples.length;
        if (last!==undefined && report.completed<last) throw Error('GPU history counter reset');
        if (last!==undefined && first>last) historyGaps+=first-last;
        for (let i=0;i<report.samples.length;i++) {
          const ordinal=first+i;
          if (last!==undefined && ordinal<last) continue;
          if (samples.length>=maxSamples) throw Error('GPU sample retention limit exceeded');
          const sample=report.samples[i]; identity(sample.identity?.selection);
          samples.push({scope:report.scope,ordinal,...sample});
        }
        completed.set(report.scope,report.completed);
        reports.set(report.scope,{...report,samples:undefined});
      }
    },
    between(after,through) { const a=identity(after),b=identity(through); return samples.filter(s=>identity(s.identity.selection)>a && identity(s.identity.selection)<=b); },
    report() { return {schema:'dimensify-browser-gpu-samples/1',historyGaps,samples:[...samples],reports:[...reports.values()],scope:'observed attributed Gaussian pass samples; partial coverage, not total frame time'}; },
  };
}
module.exports={createGpuCollector};
