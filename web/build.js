import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import compileStylus from "@1-/stylus";
import pug from "pug";

const root = import.meta.dirname,
  minify = process.argv.includes("--minify"),
  title = "fkv · performance",
  default_pug = join(root, "src", "pug", "default.pug"),
  html = pug.renderFile(default_pug, {
    filename: default_pug,
    pretty: !minify,
    title,
    body: '<div id="app"></div>',
    css_li: ["/style.css"],
    js_src_li: ["/src/main.js"],
  }),
  styl_path = join(root, "src", "styl", "_.styl"),
  css = compileStylus(readFileSync(styl_path, "utf8"), {
    filename: styl_path,
    sourcemap: false,
  }).render();

writeFileSync(join(root, "index.html"), html);
writeFileSync(join(root, "style.css"), css);
