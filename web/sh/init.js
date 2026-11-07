#!/usr/bin/env bun
import { existsSync, mkdirSync, readdirSync, rmSync, symlinkSync } from "node:fs";
import { join } from "node:path";
import yargs from "yargs";
import { hideBin } from "yargs/helpers";
import ROOT from "./ROOT.js";

const DEFAULT_LI = ["doc", "skills"],
  AGENTS = ".agents",
  symlink = (target, link_path) => {
    rmSync(link_path, { force: true, recursive: true });
    symlinkSync(target, link_path);
  },
  init = (lang) => {
    if (!existsSync(join(ROOT, "skills", lang)) && !existsSync(join(ROOT, "doc", lang))) {
      console.error("language not found: " + lang);
      process.exit(1);
    }

    const agents_dir = join(ROOT, AGENTS),
      exist_li = existsSync(agents_dir)
        ? readdirSync(agents_dir).filter((name) => !name.startsWith("."))
        : [],
      name_li = [...new Set([...DEFAULT_LI, ...exist_li])];

    mkdirSync(agents_dir, { recursive: true });

    name_li.forEach((name) => {
      const target_dir = join(ROOT, name, lang);
      if (existsSync(target_dir)) {
        symlink("../" + name + "/" + lang, join(agents_dir, name));
      }
    });

    const agents_md = join(ROOT, "skills", lang, "AGENTS.md");
    if (existsSync(agents_md)) {
      symlink("skills/" + lang + "/AGENTS.md", join(ROOT, "AGENTS.md"));
    }
  };

export default init;

if (import.meta.main) {
  const argv = yargs(hideBin(process.argv))
      .usage("usage: $0 <lang>")
      .positional("lang", {
        describe: "language",
        type: "string"
      })
      .demandCommand(1, "language is required")
      .help("h")
      .alias("h", "help")
      .parseSync(),
    lang = argv._[0] || argv.lang;

  init(lang);
}
