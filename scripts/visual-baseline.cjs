// Explicit local baseline creation. This command never changes a registry or service.
const fs = require("node:fs");
const path = require("node:path");
const { isDeepStrictEqual } = require("node:util");
const { requireValue, object, text, json, sha256, viewport, metadata, inventory,
  diskInventory, decode, validateRegions, semantic, sameInventory } = require("./visual-compare.cjs");

function argumentsFor(argv) {
  const keys = { "--actual": "actual", "--repeat": "repeat", "--expectations": "expectations", "--out": "out", "--revision": "revision", "--review": "review" };
  const options = {};
  for (let i = 0; i < argv.length; i++) {
    const key = keys[argv[i]];
    requireValue(key && !options[key] && argv[i + 1] && !argv[i + 1].startsWith("--"), `Unknown/duplicate/missing argument: ${argv[i]}`);
    const value = argv[++i];
    options[key] = ["revision", "review"].includes(key) ? value : path.resolve(value);
  }
  requireValue(Object.values(keys).every(key => text(options[key])), "Usage: --actual DIR --repeat DIR --expectations FILE --out NEWDIR --revision 40HEX --review TEXT");
  requireValue(/^[a-f0-9]{40}$/.test(options.revision), "Revision must be a full lowercase 40-hex commit");
  requireValue(options.review.trim().length > 0 && options.review.length <= 4096, "Explicit bounded review attestation is required");
  const actual = fs.realpathSync(options.actual);
  const repeat = fs.realpathSync(options.repeat);
  requireValue(actual !== repeat, "Calibration requires two distinct capture directories");
  const output = path.join(fs.realpathSync(path.dirname(options.out)), path.basename(options.out));
  for (const input of [actual, repeat, fs.realpathSync(options.expectations)])
    requireValue(output !== input && !output.startsWith(input + path.sep) && !input.startsWith(output + path.sep), "Output directory overlaps an input");
  requireValue(!fs.existsSync(options.out), "Output directory must be new");
  return options;
}
function captures(dir, revision) {
  const manifest = json(path.join(dir, "captures.json"));
  requireValue(manifest.schema === "dimensify-visual-captures/1", "Unknown capture schema");
  metadata(manifest.metadata);
  const build = manifest.metadata.build;
  requireValue(object(build) && build.commit === revision, "Capture build revision does not match requested revision");
  // Native build provenance currently encodes environment values as strings.
  requireValue(build.dirty === false || build.dirty === "false", "Capture build must attest dirty=false");
  const entries = inventory(manifest.captures, "captures");
  diskInventory(dir, manifest.captures);
  return { manifest, entries, manifestSha256: sha256(fs.readFileSync(path.join(dir, "captures.json"))) };
}
function create(options) {
  const actual = captures(options.actual, options.revision);
  const repeat = captures(options.repeat, options.revision);
  requireValue(isDeepStrictEqual(actual.manifest.metadata, repeat.manifest.metadata), "Repeat capture metadata identity mismatch");
  const expected = json(options.expectations);
  requireValue(expected.schema === "dimensify-visual-expectations/1", "Unknown expectation schema");
  viewport(expected.viewport);
  requireValue(isDeepStrictEqual(expected.viewport, actual.manifest.metadata.viewport), "Expectation viewport identity mismatch");
  const entries = inventory(expected.captures, "expectations", false);
  sameInventory(entries, actual.entries);
  sameInventory(entries, repeat.entries);
  const manifest = { schema: "dimensify-visual-baseline/1", metadata: actual.manifest.metadata,
    provenance: { reviewed: true, revision: options.revision, review: options.review },
    expectationsSha256: sha256(fs.readFileSync(options.expectations)), captures: [] };
  const images = [];
  for (const [name, entry] of entries) {
    validateRegions(entry);
    requireValue(entry.width === expected.viewport.width && entry.height === expected.viewport.height, `Expectation viewport/capture dimensions mismatch: ${name}`);
    const first = decode(options.actual, actual.entries.get(name));
    const second = decode(options.repeat, repeat.entries.get(name));
    requireValue(semantic(first.image, entry.regions).every(region => region.passed), `Actual semantic assertions failed: ${name}`);
    requireValue(semantic(second.image, entry.regions).every(region => region.passed), `Repeat semantic assertions failed: ${name}`);
    requireValue(first.image.data.equals(second.image.data), `Nonidentical decoded repeat capture: ${name}; calibration requires exact RGBA agreement`);
    const file = actual.entries.get(name).file;
    manifest.captures.push({ name, file, width: entry.width, height: entry.height, regions: entry.regions,
      sha256: sha256(first.bytes), tolerance: { channelDelta: 0, maxChangedFraction: 0, maxMeanError: 0,
        calibration: { samples: 2, description: "Two distinct capture runs have identical decoded RGBA; strict zero-error thresholds.",
          measured: { meanError: 0, changedFraction: 0, maxError: 0 },
          runs: [{ manifestSha256: actual.manifestSha256, pngSha256: sha256(first.bytes), rgbaSha256: sha256(first.image.data) },
            { manifestSha256: repeat.manifestSha256, pngSha256: sha256(second.bytes), rgbaSha256: sha256(second.image.data) }] } } });
    images.push({ file, bytes: first.bytes });
  }
  // Complete all checks before writing. The manifest is written last, so a partial
  // filesystem failure cannot leave an apparently usable baseline.
  fs.mkdirSync(options.out);
  for (const image of images) fs.writeFileSync(path.join(options.out, image.file), image.bytes, { flag: "wx" });
  fs.writeFileSync(path.join(options.out, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n", { flag: "wx" });
  return manifest;
}
function main(argv) {
  try {
    const options = argumentsFor(argv);
    const manifest = create(options);
    console.log(`Created ${manifest.captures.length} explicitly reviewed baseline captures at ${options.out}; zero-error calibration from two runs`);
    return 0;
  } catch (error) { console.error(error.message); return 1; }
}
if (require.main === module) process.exitCode = main(process.argv.slice(2));
