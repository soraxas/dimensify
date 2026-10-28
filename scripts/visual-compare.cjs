// Credential-free analytic assertions. Reference image comparison lives in Pixel Eagle.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { isDeepStrictEqual } = require("node:util");
const { PNG } = require("pngjs");

const identityKeys = ["lane", "target", "backend", "adapter", "driver", "browser", "viewport", "sceneHash", "settingsHash"];
const channels = { r: 0, g: 1, b: 2, a: 3 };
function requireValue(ok, message) { if (!ok) throw new Error(message); }
function object(value) { return value !== null && typeof value === "object" && !Array.isArray(value); }
function text(value) { return typeof value === "string" && value.trim().length > 0; }
function number(value, min, max) { return Number.isFinite(value) && value >= min && value <= max; }
function dimension(value) { return Number.isSafeInteger(value) && value > 0 && value <= 16384; }
function json(file) { return JSON.parse(fs.readFileSync(file, "utf8")); }
function sha256(bytes) { return crypto.createHash("sha256").update(bytes).digest("hex"); }
function viewport(value) {
  requireValue(object(value) && dimension(value.width) && dimension(value.height) && value.width * value.height <= 16777216, "Invalid viewport dimensions");
}
function metadata(value) {
  requireValue(object(value), "Missing metadata identity");
  for (const key of identityKeys) {
    if (key === "viewport") viewport(value[key]);
    else requireValue(text(value[key]), `Missing metadata identity ${key}`);
  }
  for (const key of ["sceneHash", "settingsHash"])
    requireValue(/^[a-f0-9]{64}$/.test(value[key]), `Invalid metadata identity ${key}`);
}
function inventory(entries, label, needsFiles = true) {
  requireValue(Array.isArray(entries) && entries.length > 0, `${label}: missing capture inventory`);
  const names = new Set();
  const files = new Set();
  for (const entry of entries) {
    requireValue(object(entry) && typeof entry.name === "string" && /^[a-z0-9][a-z0-9_-]*$/.test(entry.name), `${label}: invalid capture name`);
    requireValue(!names.has(entry.name), `${label}: duplicate capture name ${entry.name}`);
    names.add(entry.name);
    requireValue(dimension(entry.width) && dimension(entry.height) && entry.width * entry.height <= 16777216, `${label}: invalid dimensions ${entry.name}`);
    if (needsFiles) {
      requireValue(typeof entry.file === "string" && /^[a-zA-Z0-9][a-zA-Z0-9_.-]*\.png$/.test(entry.file), `${label}: unsafe PNG file ${entry.name}`);
      requireValue(!files.has(entry.file), `${label}: duplicate capture file ${entry.file}`);
      files.add(entry.file);
    }
  }
  return new Map(entries.map(entry => [entry.name, entry]));
}
function diskInventory(dir, entries) {
  const expected = entries.map(entry => entry.file).sort();
  const actual = [];
  for (const file of fs.readdirSync(dir)) {
    const stat = fs.lstatSync(path.join(dir, file));
    requireValue(!stat.isSymbolicLink(), `Symbolic link in capture directory: ${file}`);
    requireValue(!stat.isDirectory(), `Unexpected capture subdirectory: ${file}`);
    if (file.toLowerCase().endsWith(".png")) actual.push(file);
  }
  requireValue(isDeepStrictEqual(actual.sort(), expected), `PNG inventory mismatch: expected ${expected}, actual ${actual.sort()}`);
}
function decode(dir, entry) {
  const file = path.join(dir, entry.file);
  requireValue(fs.lstatSync(file).isFile(), `Not a regular PNG file: ${entry.file}`);
  const bytes = fs.readFileSync(file);
  requireValue(bytes.length >= 33 && bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) && bytes.toString("ascii", 12, 16) === "IHDR", `Invalid PNG header: ${entry.file}`);
  requireValue(bytes.readUInt32BE(16) === entry.width && bytes.readUInt32BE(20) === entry.height, `PNG dimensions mismatch: ${entry.file}`);
  const image = PNG.sync.read(bytes, { checkCRC: true });
  requireValue(image.width === entry.width && image.height === entry.height && image.data.length === entry.width * entry.height * 4, `Decoded PNG dimensions mismatch: ${entry.file}`);
  return { image, bytes };
}
function validateRegions(entry) {
  requireValue(Array.isArray(entry.regions) && entry.regions.length > 0, `Missing semantic regions for ${entry.name}`);
  const names = new Set();
  for (const region of entry.regions) {
    requireValue(object(region) && text(region.name) && !names.has(region.name), `Invalid/duplicate semantic region for ${entry.name}`);
    names.add(region.name);
    const rect = region.rect;
    requireValue(Array.isArray(rect) && rect.length === 4 && rect.every(Number.isSafeInteger) && rect[0] >= 0 && rect[1] >= 0 && rect[2] > 0 && rect[3] > 0 && rect[0] + rect[2] <= entry.width && rect[1] + rect[3] <= entry.height, `Invalid semantic region rectangle ${region.name}`);
    requireValue(Array.isArray(region.background) && region.background.length === 3 && region.background.every(v => number(v, 0, 255)), `Invalid semantic region background ${region.name}`);
    requireValue(number(region.foregroundDelta, 0, 255) && number(region.minCoverage, 0, 1) && number(region.maxCoverage, region.minCoverage, 1), `Invalid semantic coverage bounds ${region.name}`);
    requireValue(Array.isArray(region.channels), `Missing semantic channel assertions ${region.name}`);
    for (const relation of region.channels) {
      requireValue(object(relation) && Object.hasOwn(channels, relation.greater) && Object.hasOwn(channels, relation.than) && relation.greater !== relation.than && number(relation.minDelta, 0, 255), `Invalid semantic channel relationship ${region.name}`);
    }
  }
}
function semantic(image, regions) {
  return regions.map(region => {
    const [x, y, width, height] = region.rect;
    const sums = [0, 0, 0, 0];
    let foreground = 0;
    for (let row = y; row < y + height; row++) for (let col = x; col < x + width; col++) {
      const offset = (row * image.width + col) * 4;
      for (let channel = 0; channel < 4; channel++) sums[channel] += image.data[offset + channel];
      if (region.background.some((v, channel) => Math.abs(v - image.data[offset + channel]) > region.foregroundDelta)) foreground++;
    }
    const count = width * height;
    const means = sums.map(sum => sum / count);
    const coverage = foreground / count;
    const relationships = region.channels.map(relation => ({ ...relation,
      actualDelta: means[channels[relation.greater]] - means[channels[relation.than]],
      passed: means[channels[relation.greater]] - means[channels[relation.than]] >= relation.minDelta,
    }));
    return { name: region.name, coverage, mean: means,
      channels: relationships,
      passed: coverage >= region.minCoverage && coverage <= region.maxCoverage && relationships.every(relation => relation.passed),
    };
  });
}
function sameInventory(expected, actual) {
  requireValue(isDeepStrictEqual([...expected.keys()].sort(), [...actual.keys()].sort()), "Capture name inventory mismatch");
  for (const [name, entry] of expected) {
    const capture = actual.get(name);
    requireValue(entry.width === capture.width && entry.height === capture.height, `Capture dimensions mismatch: ${name}`);
  }
}
function compare(options, report) {
  const captures = json(path.join(options.actual, "captures.json"));
  report.capturesManifestSha256 = sha256(fs.readFileSync(path.join(options.actual, "captures.json")));
  requireValue(captures.schema === "dimensify-visual-captures/1", "Unknown capture schema");
  metadata(captures.metadata);
  report.actualMetadata = captures.metadata;
  const actual = inventory(captures.captures, "actual");
  diskInventory(options.actual, captures.captures);
  const manifest = json(options.expectations);
  report.expectationManifestSha256 = sha256(fs.readFileSync(options.expectations));
  requireValue(manifest.schema === "dimensify-visual-expectations/1", "Unknown expectation schema");
  viewport(manifest.viewport);
  const expectedViewport = manifest.viewport;
  const expected = inventory(manifest.captures, "expected", false);
  sameInventory(expected, actual);
  requireValue(isDeepStrictEqual(expectedViewport, captures.metadata.viewport), "Viewport identity mismatch");
  for (const [name, entry] of expected) {
    const result = { name, passed: false, errors: [] };
    report.cases.push(result);
    try {
      requireValue(entry.width === expectedViewport.width && entry.height === expectedViewport.height, `Viewport/capture dimensions mismatch: ${name}`);
      validateRegions(entry);
      const target = path.join(options.out, name);
      fs.mkdirSync(target);
      const capture = decode(options.actual, actual.get(name));
      fs.writeFileSync(path.join(target, "actual.png"), capture.bytes);
      result.regions = semantic(capture.image, entry.regions);
      result.semanticPassed = result.regions.every(region => region.passed);
      if (!result.semanticPassed) result.errors.push("Actual semantic assertions failed");
      result.passed = result.semanticPassed;
    } catch (error) { result.errors.push(error.message); }
  }
  report.passed = report.cases.length === expected.size && report.cases.every(result => result.passed);
}
function argumentsFor(argv) {
  const options = {};
  const keys = { "--actual": "actual", "--out": "out", "--expectations": "expectations" };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--semantic-only") {
      requireValue(!options.semanticOnly, "Duplicate --semantic-only");
      options.semanticOnly = true;
    } else {
      const key = keys[argv[i]];
      requireValue(key && !options[key] && argv[i + 1] && !argv[i + 1].startsWith("--"), `Unknown/duplicate/missing argument: ${argv[i]}`);
      options[key] = path.resolve(argv[++i]);
    }
  }
  requireValue(options.actual && options.out && options.expectations, "Usage: --actual DIR --expectations FILE --out NEWDIR");
  // Avoid writing output under inputs, or reading inputs through an output ancestor.
  for (const input of [options.actual, options.expectations].filter(Boolean)) {
    const inputPath = fs.existsSync(input) ? fs.realpathSync(input) : input;
    const outputParent = fs.realpathSync(path.dirname(options.out));
    const outputPath = path.join(outputParent, path.basename(options.out));
    requireValue(outputPath !== inputPath && !outputPath.startsWith(inputPath + path.sep) && !inputPath.startsWith(outputPath + path.sep), "Output directory overlaps input");
  }
  fs.mkdirSync(options.out); // Existing output is an error, including stale prior results.
  return options;
}
function main(argv) {
  let options;
  try { options = argumentsFor(argv); }
  catch (error) { console.error(error.message); return 1; }
  const report = { schema: "dimensify-visual-results/1", mode: "semantic-only", goldenCompared: false, passed: false, errors: [], cases: [] };
  try { compare(options, report); }
  catch (error) { report.errors.push(error.message); }
  fs.writeFileSync(path.join(options.out, "results.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(`${report.passed ? "PASS" : "FAIL"}: ${report.mode}; ${report.cases.filter(result => result.passed).length}/${report.cases.length} captures accepted; ${path.join(options.out, "results.json")}`);
  if (!report.passed) console.error([...report.errors, ...report.cases.flatMap(result => result.errors.map(error => `${result.name}: ${error}`))].join("\n"));
  return report.passed ? 0 : 1;
}
if (require.main === module) process.exitCode = main(process.argv.slice(2));
