import Box from "./Box.js";
import I18N_URL, { CLOSE } from "./BoxX/I18N.js";
import "./BoxX/_.styl";
import { On } from "../js/On.js";
import { B, newEl } from "../js/dom.js";
import { fetchLang } from "../js/fetchLang.js";

const fLang = fetchLang(I18N_URL),
  xClose = (dialog) => {
    const x_btn = newEl("a");

    x_btn.className = "X";
    dialog.prepend(x_btn);
    On(x_btn, { click: () => dialog.close() });
    On(dialog, {
      close: fLang((i18n) => {
        x_btn.title = i18n[CLOSE];
      })
    });
    return dialog;
  },
  escClose = (dialog) => {
    On(dialog, {
      close: On(B, {
        keyup: (e) => {
          if (27 === e.keyCode) {
            const { target } = e;
            if (["INPUT", "TEXTAREA"].includes(target.tagName)) {
              target.blur();
              return;
            }
            dialog.close();
          }
        }
      })
    });
    return dialog;
  };

export const BoxX = () => {
  const dialog = Box();
  dialog.classList.add("X");
  return xClose(escClose(dialog));
};

export default () => {
  const dialog = BoxX();
  dialog.classList.add("FFF");
  return dialog;
};
