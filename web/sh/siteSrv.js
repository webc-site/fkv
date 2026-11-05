#!/usr/bin/env bun

import ROOT from "./ROOT.js";
import siteSrv from "@webc.site/cli/bin/siteSrv.js";

await siteSrv(ROOT);
