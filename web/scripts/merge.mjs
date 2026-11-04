import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const out = process.argv[2] || "web/public/perf.json",
  dir = process.argv[3] || "perf-artifacts",
  key = (run) => [run.commit, run.platform, run.metric, run.impl].join("|");

let base = { baseline: "", generated_at: 0, runs: [] };
try {
  base = JSON.parse(readFileSync(out, "utf8"));
} catch {
  // first run: keep empty base
}

const run_map = new Map(base.runs.map((run) => [key(run), run]));

const walk = (path) => {
  for (const name of readdirSync(path)) {
    const file = join(path, name);
    if (statSync(file).isDirectory()) {
      walk(file);
      continue;
    }
    if (!name.endsWith(".json")) continue;
    const data = JSON.parse(readFileSync(file, "utf8"));
    if (data.baseline) base.baseline = data.baseline;
    for (const run of data.runs || []) {
      run_map.set(key(run), run);
    }
  }
};

try {
  walk(dir);
} catch {
  // no artifacts
}

base.runs = [...run_map.values()].sort((a, b) => a.date - b.date);
base.generated_at = Math.floor(Date.now() / 1000);
writeFileSync(out, JSON.stringify(base, null, 2) + "\n");
console.log(`perf.json: ${base.runs.length} runs`);
