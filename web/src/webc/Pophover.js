import "./Pophover/_.styl";
import { cE } from "../js/dom.js";
import { On } from "../js/On.js";

const TAG = "pophover",
  TOP = "top",
  BOTTOM = "bottom",
  LEFT = "left",
  RIGHT = "right",
  POS_LI = [TOP, BOTTOM, LEFT, RIGHT],
  updatePos = (self) => {
    const pop = self.querySelector('[slot="pop"]');
    if (!pop) {
      return;
    }

    let pop_h = pop.offsetHeight,
      pop_w = pop.offsetWidth;
    if (!pop_h) {
      pop.style.display = "block";
      pop_h = pop.offsetHeight;
      pop_w = pop.offsetWidth;
      pop.style.removeProperty("display");
    }
    if (!pop_h) {
      return;
    }

    const pop_margin = parseInt(getComputedStyle(self).getPropertyValue("--pophover-margin")) || 24,
      { top, bottom: b, left, right: r, width: w } = self.getBoundingClientRect(),
      bottom = window.innerHeight - b,
      right = window.innerWidth - r,
      need_height = pop_h + pop_margin;

    if (!self._has_pos) {
      const max_dir =
        top >= need_height
          ? TOP
          : bottom >= need_height
            ? BOTTOM
            : [
                [top, TOP],
                [bottom, BOTTOM],
                [left, LEFT],
                [right, RIGHT]
              ].reduce((a, b) => (b[0] > a[0] ? b : a))[1];

      self.classList.remove(...POS_LI);
      self.classList.add(max_dir);
    }

    const is_vert = self.classList.contains(TOP) || self.classList.contains(BOTTOM);

    if (is_vert) {
      const pad = 12,
        anchor_x = left + w / 2,
        pop_half = pop_w / 2,
        win_w = document.documentElement.clientWidth || window.innerWidth,
        pop_left = anchor_x - pop_half,
        pop_right = anchor_x + pop_half;

      let shift_x = 0;
      if (pop_left < pad) {
        shift_x = pad - pop_left;
      } else if (pop_right > win_w - pad) {
        shift_x = win_w - pad - pop_right;
      }

      self.style.setProperty("--pophover-x", Math.round(shift_x) + "px");
    } else {
      self.style.removeProperty("--pophover-x");
    }
  };

cE(
  TAG,
  class extends HTMLElement {
    connectedCallback() {
      this._has_pos = POS_LI.some((p) => this.classList.contains(p));
      updatePos(this);
      this.$ = On(this, {
        pointerenter: () => updatePos(this),
        focusin: () => updatePos(this)
      });
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
      }
    }
  }
);
