import "./Tab/_.styl";
import { cE } from "../js/dom.js";
import { On } from "../js/On.js";

const TAG = "tab",
  C_ACTIVE = "A",
  A_VAL = "value",
  C_SLOT = "slot",
  groupVal = (val) => (val ? val.split("/")[0] : ""),
  toggle = (a_li, slot_li, val) => {
    const group = groupVal(val);
    a_li.forEach((a) => {
      a.classList.toggle(C_ACTIVE, a.getAttribute(A_VAL) === group);
    });
    slot_li.forEach((el) => {
      el.classList.toggle(C_ACTIVE, el.getAttribute(C_SLOT) === group);
    });
  };

export const CHANGE = "change";

export class Tab extends HTMLElement {
  get value() {
    return this.getAttribute(A_VAL);
  }

  set value(v) {
    this.setAttribute(A_VAL, v);
  }

  connectedCallback() {
    const key = this.getAttribute("key"),
      val = (key && localStorage.getItem(key)) || this.value,
      group = groupVal(val),
      nav = this.querySelector("nav"),
      a_li = nav ? Array.from(nav.querySelectorAll("a[" + A_VAL + "]")) : [],
      slot_li = Array.from(this.children).filter(
        (el) => el.tagName === "B" && el.hasAttribute(C_SLOT)
      ),
      active_a =
        (group && a_li.find((a) => a.getAttribute(A_VAL) === group)) ||
        a_li.find((a) => a.classList.contains(C_ACTIVE)) ||
        a_li[0];

    if (active_a) {
      const active_val = active_a.getAttribute(A_VAL);
      toggle(a_li, slot_li, active_val);
      this.setAttribute(A_VAL, active_val);
      if (key) {
        localStorage.setItem(key, active_val);
      }
    }

    this.$ = On(this, {
      click: (e) => {
        const a = e.target.closest("a[" + A_VAL + "]");
        if (a && a_li.includes(a)) {
          const v = a.getAttribute(A_VAL);
          toggle(a_li, slot_li, v);
          this.setAttribute(A_VAL, v);
          if (key) {
            localStorage.setItem(key, v);
          }
          const event = new CustomEvent(CHANGE);
          event.value = v;
          this.dispatchEvent(event);
        }
      }
    });
  }

  disconnectedCallback() {
    if (this.$) {
      this.$();
    }
  }
}

cE(TAG, Tab);
