import "../../Btn.js";
import { open, close } from "../../Side.js";
import { On } from "../../../js/On.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { OPEN, CLOSE } from "./I18N.js";

const C_SIDE = "c-side",
  OPEN_BTN = ".open-btn",
  LOGOUT_BTN = ".logout-btn",
  C_OPEN = "open",
  C_CLOSE = "close",
  E_CLICK = "click";

export default (root) => {
  const side = root.querySelector(C_SIDE),
    open_btn = root.querySelector(OPEN_BTN),
    logout_btn = root.querySelector(LOGOUT_BTN);

  if (side && open_btn) {
    const { classList } = side;
    let cur_i18n;

    const fLang = fetchLang(i18nUrl),
      updateBtnText = () => {
        if (cur_i18n) {
          open_btn.textContent = cur_i18n[classList.contains(C_OPEN) ? CLOSE : OPEN];
        }
      };

    fLang((i18n) => {
      cur_i18n = i18n;
      updateBtnText();
    });

    On(open_btn, {
      [E_CLICK]: () => (classList.contains(C_OPEN) ? close(side) : open(side))
    });

    On(side, {
      [C_OPEN]: updateBtnText,
      [C_CLOSE]: updateBtnText
    });

    if (logout_btn) {
      On(logout_btn, {
        [E_CLICK]: () => close(side)
      });
    }

    open(side);
  }
};
