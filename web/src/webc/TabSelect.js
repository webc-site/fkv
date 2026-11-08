import "./Select.js";
import { Tab, CHANGE } from "./Tab.js";
import "./TabSelect/_.styl";
import { cE } from "../js/dom.js";
import { On } from "../js/On.js";

const TAG = "tab-select",
  C_ACTIVE = "A",
  A_VAL = "value",
  C_SLOT = "slot",
  getSel = (el) => el.querySelector("c-select"),
  split = (val) => {
    if (!val) return ["", ""];
    const li = val.split("/");
    return li.length > 1 ? [li[0], li[1]] : ["", li[0]];
  },
  aVal = (a, key) => {
    const sel = getSel(a),
      tab = a.getAttribute(A_VAL);
    if (!sel) return tab;
    const cache_val = key && localStorage.getItem(key + "/" + tab),
      sub_val =
        cache_val && Array.from(sel.options).some((o) => o.value === cache_val)
          ? cache_val
          : sel.value;
    return tab + "/" + sub_val;
  },
  toggle = (a_li, slot_li, val) => {
    const [group, leaf] = split(val),
      group_val = group || leaf;

    a_li.forEach((a) => {
      const sel = getSel(a),
        is_active = sel ? a.getAttribute(A_VAL) === group : a.getAttribute(A_VAL) === leaf;
      if (sel && is_active) sel.value = leaf;
      a.classList.toggle(C_ACTIVE, is_active);
    });
    slot_li.forEach((el) => {
      const is_group_active = el.getAttribute(C_SLOT) === group_val;
      el.classList.toggle(C_ACTIVE, is_group_active);
      if (is_group_active) {
        Array.from(el.children).forEach((sub) => {
          if (sub.tagName === "B" && sub.hasAttribute(C_SLOT)) {
            sub.classList.toggle(C_ACTIVE, sub.getAttribute(C_SLOT) === leaf);
          }
        });
      }
    });
  };

export class TabSelect extends Tab {
  connectedCallback() {
    super.connectedCallback();

    const key = this.getAttribute("key"),
      nav = this.querySelector("nav"),
      a_li = nav ? Array.from(nav.querySelectorAll("a")) : [],
      slot_li = Array.from(this.children).filter(
        (el) => el.tagName === "B" && el.hasAttribute(C_SLOT)
      );

    a_li.forEach((a) => {
      const sel = getSel(a);
      if (!sel) return;
      const cache_val = key && localStorage.getItem(key + "/" + a.getAttribute(A_VAL));
      if (cache_val && Array.from(sel.options).some((o) => o.value === cache_val)) {
        sel.value = cache_val;
      }
    });

    const active_a = a_li.find((a) => a.classList.contains(C_ACTIVE));
    if (active_a) {
      const active_val = aVal(active_a, key);
      if (active_val) {
        toggle(a_li, slot_li, active_val);
        this.setAttribute(A_VAL, active_val);
      }
    }

    this.$select = On(this, {
      change: (e) => {
        const { target } = e;
        if (target.tagName === "SELECT") {
          const a = target.closest("a");
          if (a && a_li.includes(a)) {
            const v = a.getAttribute(A_VAL) + "/" + target.value;
            toggle(a_li, slot_li, v);
            this.setAttribute(A_VAL, v);
            if (key) {
              localStorage.setItem(key + "/" + a.getAttribute(A_VAL), target.value);
            }
            const event = new CustomEvent(CHANGE);
            event.value = v;
            this.dispatchEvent(event);
          }
        } else {
          const v = e.value;
          if (v && !v.includes("/")) {
            e.stopPropagation();
            const a = a_li.find((el) => el.getAttribute(A_VAL) === v);
            if (a) {
              const active_val = aVal(a, key);
              toggle(a_li, slot_li, active_val);
              this.setAttribute(A_VAL, active_val);
              const event = new CustomEvent(CHANGE);
              event.value = active_val;
              this.dispatchEvent(event);
            }
          }
        }
      }
    });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    if (this.$select) {
      this.$select();
    }
  }
}

cE(TAG, TabSelect);
