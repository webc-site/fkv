import { B, D, newEl } from "./dom.js";

const toast_li = [];

export const toast = (render, timeout = 9) => {
    const el = newEl("b"),
      dialog_li = D.querySelectorAll("dialog[open]"),
      body = dialog_li[dialog_li.length - 1] || B;

    let margin_top = 0;
    for (const i of toast_li) {
      if (i.parentNode === body) {
        margin_top += 14 + i.offsetHeight;
      }
    }

    el.className = "Toast animated fadeInR";
    el.style.marginTop = margin_top + "px";

    render(el);

    toast_li.push(el);
    body.appendChild(el);

    const closeToast = () => {
        el.classList.add("fadeOutR");
        setTimeout(() => {
          const idx = toast_li.indexOf(el);
          if (idx > -1) {
            toast_li.splice(idx, 1);
          }
          if (el.parentNode) {
            el.parentNode.removeChild(el);
          }
          let top = 0;
          for (const i of toast_li) {
            if (i.parentNode === body) {
              i.style.marginTop = top + "px";
              top += 14 + i.offsetHeight;
            }
          }
        }, 500);
      },
      x = newEl("i");

    el.close = closeToast;
    x.className = "x";
    x.addEventListener("click", closeToast);
    el.appendChild(x);

    if (timeout) {
      setTimeout(closeToast, timeout * 1000);
    }

    return el;
  },
  toastErr = (render, timeout = 9) => {
    const el = toast(render, timeout);
    el.classList.add("ERR");
    return el;
  };
