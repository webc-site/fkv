import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { NAV } from "./I18N.js";
import "../../NavL.js";
import "../../Nav.js";

import { newEl } from "../../../js/dom.js";

export default (root) => {
  fetchLang(i18nUrl)((i18n) => {
    const nav = i18n[NAV],
      createNavL = (is_back) => {
        const nav_l = newEl("c-nav-l");

        if (is_back) {
          const a = newEl("a");
          a.href = "/other";
          a.textContent = "Other";
          nav_l.append(a);
        } else {
          Object.entries(nav).forEach(([key, val]) => {
            const a = newEl("a");
            a.href = location.pathname + "#" + key;
            a.textContent = val;
            nav_l.append(a);
          });
        }

        return nav_l;
      };

    root.querySelectorAll(".L").forEach((container) => {
      container.innerHTML = "";
      const is_back = container.classList.contains("back"),
        is_mobile = !!container.closest(".mobile-container"),
        nav_l = createNavL(is_back);

      container.append(nav_l);

      const menu = nav_l.querySelector("c-nav-menu");
      if (menu) {
        menu.classList.add(is_mobile ? "force-mobile" : "force-desktop");
        Object.defineProperty(menu, "offsetWidth", {
          get: () => (is_mobile ? 300 : 800)
        });
      }
    });
  });
};
