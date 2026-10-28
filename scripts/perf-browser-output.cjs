const fs = require("node:fs");
const path = require("node:path");
function prepareOutput(directory) {
  const output = path.resolve(directory);
  if (fs.existsSync(output) && fs.readdirSync(output).length) throw new Error("Output directory must be empty; choose a fresh evidence directory");
  fs.mkdirSync(output, { recursive: true });
  return output;
}
module.exports = { prepareOutput };
