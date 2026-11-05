import "./Scroll.js";
import "./Nav/_.styl";
import { cE } from "../js/dom.js";
import navScroll from "../js/navScroll.js";

cE(
  "nav",
  class extends HTMLElement {
    connectedCallback() {
      const el = this.closest("nav.Nav") || this;
      if (!el.$) {
        el.$ = navScroll(el);
      }
      el._navC = (el._navC || 0) + 1;
      this._scrollEl = el;
    }

    disconnectedCallback() {
      const el = this._scrollEl;
      if (el) {
        el._navC = (el._navC || 1) - 1;
        if (el._navC <= 0 && el.$) {
          el.$();
          el.$ = null;
        }
        this._scrollEl = null;
      }
    }
  }
);
