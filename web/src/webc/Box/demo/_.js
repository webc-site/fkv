import "../../Btn.js";
import "../../Lg.js";
import Box from "../../Box.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { TITLE, DESC, OK } from "./I18N.js";
import { D, newLi } from "../../../js/dom.js";

export default () => {
  const fLang = fetchLang(i18nUrl),
    simple_btn = D.getElementById("btn-simple"),
    pop = () => {
      const dialog = Box(),
        [form, title, p, btn_ok] = newLi("form", "h3", "p", "button"),
        close = fLang((i18n) => {
          title.textContent = i18n[TITLE];
          p.textContent = i18n[DESC];
          btn_ok.textContent = i18n[OK];
        });

      form.className = "Lg";
      dialog.addEventListener("close", close);

      btn_ok.className = "Btn Main";
      btn_ok.type = "submit";

      form.append(title, p, btn_ok);
      dialog.append(form);

      form.addEventListener("submit", (e) => {
        e.preventDefault();
        dialog.close();
      });
    };

  simple_btn.addEventListener("click", pop);
  pop();
};
