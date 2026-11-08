import "./Card.js";
import "./Ing.js";
import { ING, EMPTY, VAL } from "./CardLi/const.js";
import "./CardLi/_.styl";
import { newEl, newLi, clearDom } from "../js/dom.js";

/*
加载中

空数据

成功
[
  [
    "name",
    "brief",
    "url"
  ]
]
*/
export default (loader) => {
  const el = newEl("b"),
    render = (status, data) => {
      clearDom(el);
      if (status === ING) {
        const ing = newEl("b");
        ing.className = "Ing";
        el.append(ing);
      } else if (status === VAL && data) {
        data.forEach(([name, brief, url]) => {
          const [card, h2, i] = newLi("a", "h2", "i");

          card.className = "Card";
          if (url) {
            card.href = url;
          }

          h2.textContent = name || "";
          i.textContent = brief || "";

          card.append(h2, i);
          el.append(card);
        });
      }
    };

  el.className = "CardLi";
  render(ING);

  (async () => {
    try {
      const data = await loader();
      if (data && data.length) {
        render(VAL, data);
      } else {
        render(EMPTY);
      }
    } catch {
      render(EMPTY);
    }
  })();

  return el;
};
