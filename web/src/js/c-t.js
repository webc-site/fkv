import { cE, clearDom } from "./dom.js";
import { fetchLang } from "./fetchLang.js";

const C_T = "c-t";
export const cTranSet = (el, genUrl) => {
  const fLang = fetchLang(genUrl);
  fLang((i18n) => {
    el.querySelectorAll(C_T).forEach((i) => {
      const val = i18n[i.$];
      if (val) {
        i.textContent = val;
        i.style.visibility = "visible";
      }
    });
  });
};

cE(
  "t",
  class extends HTMLElement {
    connectedCallback() {
      if (!this.$) {
        this.$ = this.textContent;
        clearDom(this);
      }
    }
  }
);
