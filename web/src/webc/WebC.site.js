import "../js/a.js";
import "./I18n.js";
import "./Nav.js";
import "./NavL.js";
import "./O.js";
import "./Scroll.js";
import "./Toast.js";
import "./WebC.js.js";
import "./WebC.ls.js";
import cNew from "./WebC.site/cNew.js";
import fLang from "./WebC.site/fLang.js";
import initRoute from "./WebC.site/url.js";
import initScroll from "./WebC.site/scroll.js";
import { NAV } from "./WebC.site/I18N.js";
import "./WebC.site/_.styl";
import { cE, newEl } from "../js/dom.js";
import "../js/rmWait.js";

const C_ING = "Ing",
  C_A = "A",
  navRender = (nav_l_card, cur_el, i18n_nav) => {
    if (cur_el) cur_el.remove();
    const el = cNew("nav-l");
    el.append(
      ...Object.entries(i18n_nav).map(([key, text]) =>
        Object.assign(newEl("a"), { href: "/" + key, textContent: text })
      )
    );
    nav_l_card.append(el);
    return el;
  };

cE(
  "webc-site",
  class extends HTMLElement {
    connectedCallback() {
      const { classList } = this,
        [scroll, i18n_el] = ["vs", "i18n"].map(cNew),
        [nav_l_card, nav_r_card] = ["nav", "nav"].map(cNew),
        nav = newEl("nav");

      nav.className = "Nav";

      let nav_l,
        sb_el,
        cur_url = "";

      classList.add(C_ING);

      nav_r_card.append(i18n_el);
      nav.append(nav_l_card, nav_r_card);
      scroll.append(nav);
      this.append(scroll);

      const [save_scroll, restore_scroll, un_scroll] = initScroll(scroll, () => cur_url),
        show = (url, el) => {
          if (cur_url !== url) return;
          sb_el = el;
          scroll.append(sb_el);
          classList.remove(C_ING);
          restore_scroll(url);
        },
        unRoute = initRoute(
          // onLoading
          (url) => {
            save_scroll(cur_url);
            cur_url = url;

            if (sb_el) {
              sb_el.remove();
              sb_el = null;
            }

            classList.add(C_ING);
            classList.toggle(C_A, !url);
          },
          // onLoaded
          show,
          // onErr
          (url, err_li) =>
            show(
              url,
              Object.assign(newEl("pre"), {
                className: "err",
                textContent: err_li.filter(Boolean).join("\n")
              })
            )
        ),
        unLang = fLang(async (i18n) => {
          nav_l = navRender(nav_l_card, nav_l, i18n[NAV]);
          classList.remove(C_ING);
          classList.toggle(C_A, !cur_url);
        });

      this.$ = () => {
        unRoute();
        unLang();
        un_scroll();
      };
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
        this.$ = null;
      }
    }
  }
);
