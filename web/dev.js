#!/usr/bin/env bun
import dev, { getArgv } from "@webc.site/cli/bin/dev.js";
import ROOT from "./sh/ROOT.js";

await dev(ROOT, getArgv());
