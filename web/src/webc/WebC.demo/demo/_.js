import WebCDemo from "../../WebC.demo.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { TITLE, DESC, BTN, ALERT } from "./I18N.js";

const fLang = fetchLang(i18nUrl);

export const CSS = [],
  HTM =
    '<main class="demo"><div style="display:flex;align-items:center;justify-content:center;width:100%;border:1px solid #ccc;border-radius:16px;padding:32px;box-sizing:border-box">演示</div></main>';

export default async (root) => {
  const el = root.querySelector("c-webc-demo");
  if (el) {
    const [init_promise, _, unMount, setMd, root_el] = WebCDemo("WebC.demo"),
      unLang = fLang((i18n, lang) => {
        root_el._cur_i18n = i18n;
        if (root_el._updateDemo) {
          root_el._updateDemo(i18n);
        }
        setMd(lang);
      }),
      oldCleanup = el.$;
    el.$ = () => {
      oldCleanup();
      unLang();
      unMount();
    };

    el.replaceWith(await init_promise);
  } else if (root.host) {
    const active_el = root.host.closest("c-webc-demo");
    if (active_el) {
      active_el._updateDemo = (i18n) => {
        const domTitle = root.querySelector("#demoTitle"),
          domDesc = root.querySelector("#demoDesc"),
          domBtn = root.querySelector("#clickBtn");

        if (domTitle) domTitle.textContent = i18n[TITLE];
        if (domDesc) domDesc.textContent = i18n[DESC];
        if (domBtn) {
          domBtn.textContent = i18n[BTN];
          domBtn.onclick = () => alert(i18n[ALERT]);
        }
      };

      if (active_el._cur_i18n) {
        active_el._updateDemo(active_el._cur_i18n);
      }
    }
  }
};
