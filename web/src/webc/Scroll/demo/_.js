import { newEl } from "../../../js/dom.js";

const init = (box, step, offset) => {
  if (box) {
    for (let i = 0; i < 36; ++i) {
      const card = newEl("b");
      [
        ["--theme-hue", (i * step) % 360],
        ["--theme-hue-next", (i * step + offset) % 360]
      ].map(([k, v]) => card.style.setProperty(k, v));
      card.innerHTML = '<span class="num">' + (i + 1) + "</span>";
      box.appendChild(card);
    }
  }
};

export default (root) => {
  [
    ["c-hs > b", 45, 25],
    ["c-vs > b", 35, 15]
  ].map(([sel, step, offset]) => init(root.querySelector(sel), step, offset));
};
