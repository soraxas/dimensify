#!/usr/bin/env node
// A real browser connection and explicit device identity are mandatory.
require("./visual-browser.cjs").main(true).catch(error=>{
  console.error(require("./visual-browser.cjs").sanitize(error));
  process.exitCode=1;
});
