#!/usr/bin/env bun

import ROOT from "./ROOT.js";
import dist from "@webc.site/cli/bin/dist.js";
await dist(ROOT);
