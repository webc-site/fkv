import { On } from "../../../js/On.js";

const cardBind = (el) => {
  const CARD = ".Card",
    A = "A",
    CARD_A = CARD + "." + A;

  On(el, {
    click: (e) => {
      const card = e.target.closest(CARD);
      if (card) {
        const active = el.querySelector(CARD_A);
        if (active && active !== card) {
          active.classList.remove(A);
        }
        card.classList.add(A);
      }
    }
  });
};
export default (root) => {
  const box = root.querySelector(".demo .card-box");
  if (box) {
    cardBind(box);
  }
};
