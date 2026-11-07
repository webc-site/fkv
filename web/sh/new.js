#!/usr/bin/env node
import ROOT from "./ROOT.js";
import run from "@webc.site/cli/bin/new.js";
process.chdir(ROOT);
const code = run();
if (code) {
  process.exit(code);
}
