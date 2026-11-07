import "./Select.js";
import "./SelectBtn/_.styl";
import { cE } from "../js/dom.js";
import { On } from "../js/On.js";

const TAG = "select-btn",
  A_KEY = "key",
  A_VAL = "value",
  E_SUBMIT = "submit",
  init = (self) => {
    if (!self.isConnected || self.$) return;

    const sel = self.querySelector("c-select"),
      btn = self.querySelector("button");

    if (!sel || !btn) return;

    const key = self.getAttribute(A_KEY),
      val = (key && localStorage.getItem(key)) || self.getAttribute(A_VAL);

    if (val) {
      sel.value = val;
      self.setAttribute(A_VAL, val);
    }

    const change = () => {
        const v = sel.value;
        self.setAttribute(A_VAL, v);
        if (key) {
          localStorage.setItem(key, v);
        }
      },
      submit = (e) => {
        e.stopPropagation();
        const evt = new CustomEvent(E_SUBMIT);
        evt.value = sel.value;
        self.dispatchEvent(evt);
      };

    self.$ = On(self, {
      change: ({ target }) => target.closest("c-select") === sel && change(),
      click: (e) => e.target === btn && submit(e)
    });
  };

cE(
  TAG,
  class extends HTMLElement {
    get value() {
      const sel = this.querySelector("c-select");
      return sel ? sel.value : "";
    }

    connectedCallback() {
      queueMicrotask(() => init(this));
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
        this.$ = null;
      }
    }
  }
);
