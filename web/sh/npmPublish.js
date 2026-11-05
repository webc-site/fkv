#!/usr/bin/env bun

import ROOT from "./ROOT.js";
import npmPublish from "@webc.site/cli/bin/npmPublish.js";

await npmPublish(ROOT);
