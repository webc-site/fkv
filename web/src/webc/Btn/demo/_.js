import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { ADD } from "./I18N.js";

export default (root) => {
  const fLang = fetchLang(i18nUrl),
    btn = root.querySelector(".demo .BtnC");

  if (btn) {
    fLang((i18n) => {
      btn.setAttribute("aria-label", i18n[ADD]);
    });
  }
};
