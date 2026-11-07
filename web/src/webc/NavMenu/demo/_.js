import { newEl } from "../../../js/dom.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { NAV } from "./I18N.js";
import "../../NavMenu.js";
import "../../Nav.js";

export default (root) => {
  fetchLang(i18nUrl)((i18n) => {
    const nav = i18n[NAV];
    root.querySelectorAll("b.L").forEach((container) => {
      container.innerHTML = "";
      const menu = newEl("c-nav-menu"),
        is_mobile = container.closest(".mobile-container") !== null;
      menu.classList.add(is_mobile ? "force-mobile" : "force-desktop");
      Object.defineProperty(menu, "offsetWidth", {
        get: () => (is_mobile ? 300 : 800)
      });
      Object.entries(nav).forEach(([k, v]) => {
        const a = newEl("a");
        a.href = location.pathname + "#" + k;
        a.textContent = v;
        menu.append(a);
      });
      container.append(menu);
    });
  });
};
