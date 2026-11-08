import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { MD_CONTENT } from "./I18N.js";

export default (root) => {
  const fLang = fetchLang(i18nUrl),
    static_el = root.querySelector("c-md.static");

  fLang((i18n) => {
    const md_content = i18n[MD_CONTENT];
    if (static_el) {
      static_el.value = md_content;
    }
  });
};
