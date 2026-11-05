import "./MdCss.js";
import "./Md/_.styl";
import { cE } from "../js/dom.js";
import render from "../js/md/safeHtm.js";

cE(
  "md",
  class extends HTMLElement {
    get value() {
      return this._value ?? "";
    }

    set value(val) {
      this._value = val;
      this.innerHTML = render(val);
    }

    connectedCallback() {
      this.classList.add("Md");
      let val;
      if (this.hasOwnProperty("value")) {
        val = this.value;
        delete this.value;
      } else {
        val = this.textContent;
      }
      if (val) this.value = val;
    }
  }
);
