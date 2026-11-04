// Diagnostic repeat comparison only; no baseline selection or acceptance tolerance.
// This helper targets the portable renderer testbed's five 256x256 cases,
// matching the bounded artifact/uploader inventory rather than arbitrary scenes.
const fs = require("node:fs");
const path = require("node:path");
const { isDeepStrictEqual } = require("node:util");
const { PNG } = require("pngjs");
function pixelStats(a, b) {
  if (a.length !== b.length || a.length % 4) throw Error("Pixel dimensions differ");
  let changedPixels = 0, changedChannels = 0, maxChannelDelta = 0, total = 0;
  for (let offset = 0; offset < a.length; offset += 4) {
    let changed = false;
    for (let channel = 0; channel < 4; channel++) {
      const delta = Math.abs(a[offset + channel] - b[offset + channel]);
      if (delta) { changed = true; changedChannels++; }
      maxChannelDelta = Math.max(maxChannelDelta, delta); total += delta;
    }
    if (changed) changedPixels++;
  }
  return { pixels: a.length / 4, changedPixels, changedChannels, maxChannelDelta, meanAbsoluteChannelDelta: total / a.length };
}
function manifest(directory) {
  const file = path.join(directory, "captures.json");
  if (fs.lstatSync(file).isSymbolicLink() || fs.statSync(file).size > 65536) throw Error("Invalid capture manifest");
  const value = JSON.parse(fs.readFileSync(file, "utf8"));
  if (value.schema !== "dimensify-visual-captures/1" || !Array.isArray(value.captures) || value.captures.length !== 5) throw Error("Invalid capture inventory");
  const names = new Set();
  for (const entry of value.captures) {
    if (!/^[a-z][a-z-]*$/.test(entry.name) || entry.file !== entry.name + ".png" || entry.width !== 256 || entry.height !== 256 || names.has(entry.name)) throw Error("Invalid capture identity");
    names.add(entry.name);
  }
  return value;
}
function pixels(directory, entry) {
  const file = path.join(directory, entry.file);
  if (fs.lstatSync(file).isSymbolicLink() || fs.statSync(file).size > 4 * 1024 * 1024) throw Error("Invalid capture image");
  const bytes = fs.readFileSync(file);
  if (bytes.length < 33 || bytes.readUInt32BE(16) !== 256 || bytes.readUInt32BE(20) !== 256) throw Error("Invalid capture dimensions");
  const png = PNG.sync.read(bytes, { checkCRC: true });
  if (png.width !== 256 || png.height !== 256) throw Error("Invalid decoded dimensions");
  return png.data;
}
function compare(first, second) {
  const a = manifest(first), b = manifest(second);
  for (const key of ["lane", "target", "backend", "adapter", "driver", "browser", "viewport", "sceneHash", "settingsHash"]) {
    if (a.metadata[key] === undefined || !isDeepStrictEqual(a.metadata[key], b.metadata[key])) throw Error("Repeat identity mismatch: " + key);
  }
  const cases = a.captures.map(entry => {
    const other = b.captures.find(row => row.name === entry.name);
    if (!other) throw Error("Repeat inventory mismatch");
    return { name: entry.name, ...pixelStats(pixels(first, entry), pixels(second, other)) };
  });
  return { schema: "dimensify-visual-repeat/1", diagnosticOnly: true, goldenCompared: false,
    identical: cases.every(row => row.changedPixels === 0), firstMetadata: a.metadata, secondMetadata: b.metadata, cases };
}
if (require.main === module) {
  try {
    const [first, second, output] = process.argv.slice(2);
    if (!first || !second || !output) throw Error("Usage: node scripts/visual-repeat.cjs FIRST SECOND NEW_REPORT.json");
    const result = compare(first, second);
    fs.writeFileSync(output, JSON.stringify(result, null, 2) + "\n", { flag: "wx" });
    console.log("Repeat capture diagnostics: " + (result.identical ? "identical" : "variation observed; inspect " + output));
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { pixelStats, compare };
