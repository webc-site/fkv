import { On } from "../../../js/On.js";

export default (root) => {
  const tabali = root.querySelector(".NavALi");
  if (tabali) {
    const updateActive = () => {
      const hash = location.hash || "#home";
      tabali.querySelectorAll(".Card").forEach((el) => {
        const href = el.getAttribute("href");
        el.classList.toggle("A", href === hash);
      });
    };

    updateActive();

    const off = On(window, {
      hashchange: () => {
        if (!root.contains(tabali)) {
          off();
          return;
        }
        updateActive();
      }
    });
  }
};
