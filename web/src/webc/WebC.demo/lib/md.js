import renderHtml from "../../../js/md/htm.js";
import { newEl, newLi } from "../../../js/dom.js";
import JSD from "./JSD.js";
import MOD from "./MOD.js";

const HTML = 1,
  JS = 2;

export default (h1, b, doc_h2, url) => {
  const doc_md = newEl("div");
  doc_md.className = "Md";

  const titleRender = (name, doc_title, lang) => {
      h1.textContent = name;
      const [span, edit_btn] = newLi("span", "c-edit");
      span.textContent = doc_title;
      edit_btn.setAttribute("href", DOC_EDIT + "src/webc/" + url + "/i18n/" + lang + "/README.md");
      doc_h2.replaceChildren(span, edit_btn);
    },
    mdRender = (name, md, mod) => {
      const line_li = md.split("\n"),
        title_idx = line_li.findIndex((line) => line.startsWith("# ")),
        has_title = title_idx !== -1,
        brief = has_title ? line_li[title_idx].slice(2) : name,
        content = has_title
          ? line_li
              .slice(title_idx + 1)
              .join("\n")
              .trim()
          : md,
        { HAS_I18N } = mod,
        css_li = [],
        js_li = HAS_I18N ? ['import "' + MOD + 'I18n.js";'] : [];

      b.textContent = brief;

      doc_md.innerHTML = renderHtml(content).replace(
        /<pre><code class="language-(html|js)">([\s\S]*?)<\/code><\/pre>/g,
        '<c-htm-demo><pre><code class="language-$1">$2</code></pre></c-htm-demo>'
      );

      doc_md.querySelectorAll("c-htm-demo").forEach((el) => {
        el.gen = (type, text) => {
          const css = css_li.map((x) => "@import '" + JSD + x + "_.css';").join("\n"),
            js_dep_li = !text.includes("</script>")
              ? ['import "' + MOD + url + '.js";', ...js_li]
              : js_li;
          let body = "",
            js = js_dep_li.join("\n");

          if (type === JS) {
            js = (js ? js + "\n" : "") + text;
          } else if (type === HTML) {
            body = text;
          }

          if (HAS_I18N) {
            body = "<c-i18n></c-i18n>\n" + body;
          }

          const head =
            '<link rel="stylesheet" href="' +
            JSD +
            '_.css">\n' +
            '<script type="importmap">{"imports":{"' +
            MOD +
            '":"' +
            JSD +
            '"}}</script>';

          return [head, body, css, js];
        };
      });
    };

  return [titleRender, mdRender, doc_md];
};
