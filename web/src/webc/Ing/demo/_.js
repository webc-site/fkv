import "../../Btn.js";
import "../../Lg.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { USER_PLACEHOLDER, PASS_PLACEHOLDER } from "./I18N.js";
import { D } from "../../../js/dom.js";

export default () => {
  const fLang = fetchLang(i18nUrl),
    [ipt_user, ipt_pass] = ["ipt-user", "ipt-pass"].map((i) => D.getElementById(i));

  fLang((i18n) => {
    [
      [ipt_user, USER_PLACEHOLDER],
      [ipt_pass, PASS_PLACEHOLDER]
    ].forEach(([ipt, pl]) => {
      if (ipt) ipt.placeholder = i18n[pl];
    });
  });
};
