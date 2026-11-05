import "../../Btn.js";
import { toast, toastErr } from "../../../js/Toast.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { MSG_INFO, MSG_ERR } from "./I18N.js";
import { D } from "../../../js/dom.js";

export default () => {
  const fLang = fetchLang(i18nUrl),
    [btn, btn_err, ipt_timeout] = ["btn-toast", "btn-toast-err", "ipt-timeout"].map((i) =>
      D.getElementById(i)
    ),
    conf = () => {
      return ipt_timeout ? Number(ipt_timeout.value) : 9;
    };

  [
    [btn, toast, MSG_INFO],
    [btn_err, toastErr, MSG_ERR]
  ].map(([btn_el, fn, msg]) => {
    if (btn_el) {
      btn_el.addEventListener("click", () => {
        fn((el) => {
          fLang((i18n) => {
            el.textContent = i18n[msg];
          });
        }, conf());
      });
    }
  });
};
