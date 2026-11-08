import { env } from "node:process";
import { execSync } from "node:child_process";

let rev;
for (const name of ["CF_PAGES_COMMIT_SHA", "GITHUB_SHA", "VERCEL_GIT_COMMIT_SHA"]) {
  const val = env[name];
  if (val) {
    rev = val.slice(0, 7);
    break;
  }
}

if (!rev) {
  try {
    rev = execSync("git rev-parse --short HEAD", {
      stdio: ["ignore", "pipe", "ignore"]
    })
      .toString()
      .trim();
  } catch {
    rev = "main";
  }
}

export default rev;
