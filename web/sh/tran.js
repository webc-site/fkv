#!/usr/bin/env bun
import { chdir } from "process";
import tran from "@webc.site/cli/bin/tran.js";
import ROOT from "./ROOT.js";

chdir(ROOT);

await tran();
