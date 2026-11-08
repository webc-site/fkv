import "./Edit.js";
import "./Code.js";
import "./Md.js";
import "./H1Txt.js";
import i18nUrl, { DOC, CODE } from "./WebC.js/I18N.js";
import { docUrl, srcUrl } from "./WebC.js/lib/url.js";
import "./WebC.js/_.styl";
import { cE, newEl, newLi } from "../js/dom.js";
import headerNew from "../js/dom/header.js";
import { fetchLang } from "../js/fetchLang.js";
import { onLang } from "../js/i18n.js";
import { fTxt } from "../js/fetch.js";

const C_ING = "Ing",
  fLang = fetchLang(i18nUrl),
  cardNew = (div_class, span, btn, card_el, card_class) => {
    const sec = newEl("b"),
      h2 = newEl("h2"),
      div = newEl("b");
    h2.className = "O";
    h2.append(span, btn);
    card_el.className = card_class;
    sec.append(h2, card_el);
    div.className = div_class;
    div.append(sec);
    return div;
  };

cE(
  "webc-js",
  class extends HTMLElement {
    connectedCallback() {
      if (this.firstChild) return;

      const [header, h1, b] = headerNew(),
        [main_div, doc_span, edit_btn, doc_md, code_span, code_btn, doc_card, code_card] = newLi(
          "b",
          "span",
          "c-edit",
          "c-md",
          "span",
          "c-code",
          "main",
          "main"
        ),
        rel = this.getAttribute("rel");

      doc_card.append(doc_md);

      const left_div = cardNew("L", doc_span, edit_btn, doc_card, "Lg"),
        right_div = cardNew("R", code_span, code_btn, code_card, "Lg CodePre");

      main_div.className = "M";
      main_div.append(left_div, right_div);

      this.append(header, main_div);

      h1.textContent = location.pathname.slice(4) + ".js";

      this.$ = onLang(async (lang_code) => {
        fLang((i18n) => {
          doc_span.textContent = i18n[DOC];
          code_span.textContent = i18n[CODE];
        });

        edit_btn.setAttribute("href", DOC_EDIT + "doc/" + lang_code + "/js/" + rel + ".md");
        code_btn.setAttribute("href", GIT_SRC_BLOB + "js/" + rel + ".js");

        const loadDoc = async () => {
            doc_card.classList.add(C_ING);
            try {
              const md_text = await fTxt(docUrl(rel, lang_code)),
                line_li = md_text.split("\n"),
                title = line_li[0].slice(2).trim(),
                content = line_li.slice(1).join("\n").trim(),
                content_li = content.split("\n"),
                brief_idx = content_li.findIndex((line) => line.trim().length > 0);

              let brief = "",
                body = content;

              if (brief_idx !== -1) {
                brief = content_li[brief_idx].trim();
                const rest_body = content_li
                  .slice(brief_idx + 1)
                  .join("\n")
                  .trim();
                if (rest_body.length > 0) {
                  body = rest_body;
                }
              }

              h1.textContent = title;
              b.textContent = brief;
              doc_md.value = body;
            } finally {
              doc_card.classList.remove(C_ING);
            }
          },
          loadCode = async () => {
            code_card.classList.add(C_ING);
            try {
              const src_text = await fTxt(srcUrl(rel));
              code_card.textContent = src_text;
            } finally {
              code_card.classList.remove(C_ING);
            }
          };

        await Promise.all([loadDoc(), loadCode()]);
      });
    }

    disconnectedCallback() {
      this.$();
    }
  }
);
