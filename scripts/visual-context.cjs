// Supplemental capture diagnostics. These fields do not select/approve references.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const rendererKeys = ["LP_NATIVE_VECTOR_WIDTH", "LP_NUM_THREADS", "GALLIUM_OVERRIDE_CPU_CAPS", "GALLIVM_PERF", "MESA_SHADER_CACHE_DISABLE", "WGPU_BACKEND"];
function cpuFlags(cpuinfo) {
  const rows = cpuinfo.split("\n").filter(line => /^(flags|Features)\s*:/.test(line));
  if (!rows.length) return null;
  return [...new Set(rows.flatMap(row => row.slice(row.indexOf(":") + 1).trim().split(/\s+/)))].sort();
}
function collect(env = process.env) {
  let flags = null;
  if (process.platform === "linux") {
    try { flags = cpuFlags(fs.readFileSync("/proc/cpuinfo", "utf8")); } catch { /* Explicitly unavailable. */ }
  }
  return { schema: "dimensify-visual-host/1", system: os.platform(), release: os.release(), arch: os.arch(),
    cpu_models: [...new Set(os.cpus().map(cpu => cpu.model))].sort(), cpu_flags: flags,
    logical_cpus: os.cpus().length, runner_image: env.ImageVersion || null,
    renderer_environment: Object.fromEntries(rendererKeys.map(key => [key, env[key] || null])) };
}
function attach(directory) {
  const file = path.join(directory, "captures.json");
  if (fs.lstatSync(file).isSymbolicLink() || fs.statSync(file).size > 65536) throw Error("Invalid capture manifest");
  const manifest = JSON.parse(fs.readFileSync(file, "utf8"));
  if (manifest.schema !== "dimensify-visual-captures/1" || !manifest.metadata) throw Error("Unknown capture manifest");
  manifest.metadata.captureHost = collect();
  fs.writeFileSync(file, JSON.stringify(manifest, null, 2) + "\n");
}
if (require.main === module) {
  try {
    if (process.argv.length !== 3) throw Error("Usage: node scripts/visual-context.cjs CAPTURES");
    attach(process.argv[2]);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
module.exports = { collect, cpuFlags, attach };
