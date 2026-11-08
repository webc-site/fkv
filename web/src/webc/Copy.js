import I18N_URL, { COPY, DONE } from "./Copy/I18N.js";
import "./Pophover.js";
import "./Copy/_.styl";
import { cE, newLi } from "../js/dom.js";
import { On } from "../js/On.js";
import { fetchLang } from "../js/fetchLang.js";

const C_DONE = "Done",
  fLang = fetchLang(I18N_URL),
  targetEl = (self, rel) => {
    if (!rel) {
      return self.previousElementSibling;
    }
    return self.getRootNode().querySelector(rel);
  };

cE(
  "copy",
  class extends HTMLElement {
    connectedCallback() {
      if (this.firstChild) return;

      const rel = this.getAttribute("rel"),
        [pophover, btn, tip] = newLi("c-pophover", "a", "div");

      btn.className = "BtnIco";

      tip.setAttribute("slot", "pop");

      pophover.append(btn, tip);
      this.append(pophover);

      let timer,
        text_copy = "",
        text_done = "";

      const unLang = fLang((i18n) => {
          text_copy = i18n[COPY];
          text_done = i18n[DONE];
          if (!pophover.classList.contains(C_DONE)) {
            tip.textContent = text_copy;
          }
        }),
        reset = () => {
          if (pophover.classList.contains(C_DONE)) {
            pophover.classList.remove(C_DONE);
            tip.textContent = text_copy;
            clearTimeout(timer);
          }
        },
        handleCopy = () => {
          const target = targetEl(this, rel);
          if (!target) return;

          navigator.clipboard.writeText(target.innerText || "");

          pophover.classList.add(C_DONE);
          tip.textContent = text_done;

          clearTimeout(timer);
          timer = setTimeout(() => {
            pophover.classList.remove(C_DONE);
            tip.textContent = text_copy;
          }, 2000);
        },
        un = On(btn, {
          click: handleCopy,
          mouseenter: reset
        });

      this.$ = () => {
        unLang();
        un();
        clearTimeout(timer);
      };
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
      }
    }
  }
);
