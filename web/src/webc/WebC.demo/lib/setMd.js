import { newEl } from "../../../js/dom.js";
import { fTxt } from "../../../js/fetch.js";
import readmeUrl from "./readmeUrl.js";

const ING = "Ing";

export default (init_promise, modGet, titleRender, mdRender, url, doc_md) => {
  let cur_lang,
    cur_title,
    cur_md_promise,
    ing_cnt = 0;

  const doc_div = newEl("main"),
    class_list = doc_div.classList,
    render = async (lang) => {
      if (!cur_title || !cur_md_promise || cur_lang !== lang) return;

      titleRender(url, cur_title, lang);

      if (!ing_cnt) {
        class_list.add(ING);
      }
      ++ing_cnt;
      try {
        await init_promise;
        if (cur_lang !== lang) return;

        const mod = modGet(),
          md = await cur_md_promise;
        if (cur_lang !== lang) return;

        mdRender(url, md, mod);
      } finally {
        --ing_cnt;
        if (!ing_cnt) {
          class_list.remove(ING);
        }
      }
    },
    setDocTitle = (title, lang) => {
      if (cur_lang !== lang) {
        cur_lang = lang;
        cur_md_promise = 0;
      }
      cur_title = title;
      render(lang);
    },
    setMd = (lang) => {
      if (cur_lang !== lang) {
        cur_lang = lang;
        cur_title = 0;
      }
      cur_md_promise = fTxt(readmeUrl(url, lang));
      render(lang);
    };

  doc_div.id = "doc";
  doc_div.className = "Lg";
  doc_div.append(doc_md);

  return [setMd, setDocTitle, doc_div];
};
