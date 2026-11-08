import { onLang, LANG_LI } from "../../../js/i18n.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { LANGUAGE, INDEX } from "./I18N.js";
import { D } from "../../../js/dom.js";

export default () => {
  const fLang = fetchLang(i18nUrl);

  onLang((_, val) => {
    const text = D.getElementById("lang"),
      idx = val ?? 1,
      item = LANG_LI[idx];

    if (text) {
      fLang((i18n) => {
        text.innerText =
          (item ? item[0] : i18n[LANGUAGE] + " " + idx) + " (" + i18n[INDEX] + ": " + idx + ")";
      });
    }
  });
};
