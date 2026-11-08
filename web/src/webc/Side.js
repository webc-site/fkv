import "./Scroll.js";
import "./Side/_.styl";
import { cE, newLi } from "../js/dom.js";
import { On } from "../js/On.js";

const TAG = "side",
  C_H = "H",
  C_T = "T",
  C_RIGHT = "right",
  C_TOP = "top",
  C_BOTTOM = "bottom",
  E_OPEN = "open",
  E_CLOSE = "close";

export const close = (self) => self.close(),
  open = (self) => self.open();

cE(
  TAG,
  class extends HTMLElement {
    connectedCallback() {
      const { classList } = this;

      if (!this.querySelector(".In")) {
        const [drawer, scroll] = newLi("div", "c-vs");

        drawer.className = "In";

        while (this.firstChild) {
          scroll.append(this.firstChild);
        }
        drawer.append(scroll);

        this.append(drawer);
      }

      if (!classList.contains(E_OPEN) && !classList.contains(C_T)) {
        classList.add(C_H);
      }

      let start_x = 0,
        start_y = 0,
        un_win = () => {},
        timer = 0;

      const close = () => {
          if (!classList.contains(E_OPEN)) {
            classList.remove(C_T);
            classList.add(C_H);
            return;
          }
          classList.remove(E_OPEN);
          classList.add(C_T);
          this.dispatchEvent(new CustomEvent(E_CLOSE));
          un_win();
          un_win = () => {};

          clearTimeout(timer);
          timer = setTimeout(() => {
            timer = 0;
            if (!classList.contains(E_OPEN)) {
              classList.remove(C_T);
              classList.add(C_H);
            }
          }, 300);
        },
        open = () => {
          clearTimeout(timer);
          timer = 0;
          classList.remove(C_H, C_T);
          classList.add(E_OPEN);
          this.dispatchEvent(new CustomEvent(E_OPEN));
          un_win();
          setTimeout(() => {
            if (this.isConnected && classList.contains(E_OPEN)) {
              un_win = On(window, {
                click: (e) => {
                  if (!this.contains(e.target)) {
                    close();
                  }
                }
              });
            }
          }, 0);
        },
        onStart = (e) => {
          const t = e.touches[0];
          if (t) {
            const { clientX, clientY } = t;
            start_x = clientX;
            start_y = clientY;
          }
        },
        onEnd = (e) => {
          const t = e.changedTouches[0];
          if (t) {
            const { clientX, clientY } = t,
              dx = clientX - start_x,
              dy = clientY - start_y,
              abs_x = Math.abs(dx),
              abs_y = Math.abs(dy);

            if (classList.contains(C_TOP)) {
              if (abs_y > abs_x * 1.5 && abs_y > 50 && dy < 0) close();
            } else if (classList.contains(C_BOTTOM)) {
              if (abs_y > abs_x * 1.5 && abs_y > 50 && dy > 0) close();
            } else if (abs_x > abs_y * 1.5 && abs_x > 50) {
              if (classList.contains(C_RIGHT) ? dx > 0 : dx < 0) close();
            }
          }
        },
        unEvent = On(this, {
          touchstart: onStart,
          touchend: onEnd
        });

      this.close = close;
      this.open = open;

      this.$ = () => {
        unEvent();
        un_win();
        clearTimeout(timer);
        timer = 0;
        this.close = 0;
        this.open = 0;
      };
    }

    disconnectedCallback() {
      this.$();
    }
  }
);
