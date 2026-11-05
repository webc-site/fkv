import I18N_URL, { BACK } from "./Back/I18N.js";
import "./Back/_.styl";
import { cE, newLi } from "../js/dom.js";
import { fetchLang } from "../js/fetchLang.js";

const fLang = fetchLang(I18N_URL);

cE(
  "back",
  class extends HTMLElement {
    static get observedAttributes() {
      return ["href"];
    }

    attributeChangedCallback(name, _, new_val) {
      if (name === "href") {
        const btn = this.querySelector("a");
        if (btn) {
          btn.href = new_val || "";
        }
      }
    }

    connectedCallback() {
      if (this.firstChild) return;

      const href = this.getAttribute("href"),
        [btn, icon] = newLi("a", "i");

      btn.className = "BtnC back";
      if (href) {
        btn.href = href;
      }

      icon.className = "Ico";
      btn.append(icon);

      this.append(btn);

      this.$ = fLang((i18n) => {
        const val = i18n[BACK];
        btn.title = val;
        btn.setAttribute("aria-label", val);
      });
    }

    disconnectedCallback() {
      this.$();
    }
  }
);
