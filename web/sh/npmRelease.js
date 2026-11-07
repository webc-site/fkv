#!/usr/bin/env bun

import ROOT from "./ROOT.js";
import npmRelease from "@webc.site/cli/bin/npmRelease.js";

await npmRelease(ROOT);
