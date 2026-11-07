#!/usr/bin/env bun

import ROOT from "./ROOT.js";
import distSrv from "@webc.site/cli/bin/distSrv.js";

await distSrv(ROOT);
