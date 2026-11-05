import "./NavALi.js";
import { open as openSide, close as closeSide } from "./Side.js";
import "./NavMenu/_.styl";
import { cE, newLi } from "../js/dom.js";
import { On } from "../js/On.js";
import { route } from "../js/route.js";

const TAG = "nav-menu",
  C_ACTIVE = "A",
  E_CLICK = "click",
  update = (list) => {
    if (!list) return;
    const { pathname, hash } = location,
      items = Array.from(list.querySelectorAll("a")),
      has_hash_match = hash && items.some((a) => a.pathname === pathname && a.hash === hash);

    items.forEach((a) => {
      const { pathname: a_path, hash: a_hash, classList: class_list } = a,
        active = a_path === pathname && (has_hash_match ? a_hash === hash : !a_hash);
      class_list.toggle(C_ACTIVE, active);
    });
  };

cE(
  TAG,
  class extends HTMLElement {
    connectedCallback() {
      let side = this.querySelector("c-side"),
        mb_btn,
        list;

      if (side) {
        mb_btn = side.previousElementSibling;
        list = side.querySelector("b");
      } else {
        let ico;
        [ico, mb_btn, side, list] = newLi("i", "button", "c-side", "b");

        mb_btn.className = "BtnC";
        ico.className = "Ico";
        mb_btn.append(ico);

        Array.from(this.querySelectorAll("a")).forEach((a) => {
          a.classList.add("Card");
          list.append(a);
        });

        side.append(list);
        this.append(mb_btn, side);
      }

      const { classList } = list,
        media = window.matchMedia("(min-width: 701px)"),
        onMedia = (e) => classList.toggle("NavALi", e.matches),
        updateList = update.bind(null, list),
        calls = [
          On(mb_btn, { [E_CLICK]: () => openSide(side) }),
          On(list, {
            [E_CLICK]: (e) => e.target.closest("a") && closeSide(side)
          }),
          route(updateList),
          On(window, {
            hashchange: updateList,
            popstate: updateList
          }),
          On(media, { change: onMedia })
        ];

      onMedia(media);

      updateList();

      this.$ = () => calls.forEach((f) => f());
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
        this.$ = null;
      }
    }
  }
);
