import { newEl } from "../../../js/dom.js";
export default (root) => {
  const content = root.querySelector(".content"),
    target_h = window.innerHeight * 3;
  let h = 0,
    i = 0;
  while (h < target_h) {
    const card = newEl("b"),
      hue = (i * 37) % 360;

    card.style.background =
      "radial-gradient(circle at 20% 0%,hsla(" +
      hue +
      ",75%,90%,0.5),transparent 75%)," +
      "linear-gradient(135deg,hsla(" +
      hue +
      ",60%,95%,0.4),hsla(" +
      hue +
      ",40%,98%,0.2))";
    card.style.borderColor = "hsla(" + hue + ",60%,78%,0.9)";
    content.appendChild(card);
    h += 96;
    ++i;
  }
};
