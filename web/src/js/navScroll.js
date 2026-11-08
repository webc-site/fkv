import parentScroll from "./parentScroll.js";
import { On } from "./On.js";

const C_D = "D",
  C_H = "H",
  C_I = "I";

export default (el) => {
  if (!el) return () => {};

  const scroll = parentScroll(el);
  if (!scroll) return () => {};

  let last_y = scroll.scrollTop,
    start_y = last_y,
    ticking = 0,
    timer = null;

  const { classList } = el,
    rmTimer = () => {
      if (timer) {
        clearTimeout(timer);
        timer = null;
      }
    },
    hide = (anim) => {
      rmTimer();
      if (anim) {
        classList.remove(C_D);
        classList.add(C_I);
        timer = setTimeout(() => {
          timer = null;
          if (!classList.contains(C_D)) {
            classList.remove(C_I);
            classList.add(C_H);
          }
        }, 300);
      } else {
        classList.remove(C_D, C_I);
        classList.add(C_H);
      }
    },
    show = () => {
      rmTimer();
      classList.remove(C_I, C_H);
      classList.add(C_D);
    },
    reset = () => {
      rmTimer();
      classList.remove(C_D, C_I, C_H);
    },
    update = () => {
      const { scrollTop, scrollHeight, clientHeight } = scroll,
        max_y = scrollHeight - clientHeight,
        { offsetHeight } = el;

      if (max_y <= 0) {
        reset();
        return;
      }

      const y = Math.max(0, Math.min(scrollTop, max_y));
      if (y <= offsetHeight) {
        reset();
        start_y = y;
      } else if (y > last_y) {
        if (classList.contains(C_D)) {
          hide(true);
        } else if (!classList.contains(C_H)) {
          hide(false);
        }
        start_y = y;
      } else if (y < last_y) {
        if (classList.contains(C_D)) {
          start_y = y;
        } else if (start_y - y > 80) {
          show();
        }
      }
      last_y = y;
    },
    onScroll = () => {
      if (!ticking) {
        ticking = requestAnimationFrame(() => {
          update();
          ticking = 0;
        });
      }
    },
    ob = new ResizeObserver(() => {
      const { offsetHeight, style } = el,
        { scrollTop } = scroll;
      style.setProperty("--top", -offsetHeight + "px");
      last_y = scrollTop;
      start_y = last_y;
      if (last_y === 0) {
        reset();
      }
    }),
    { firstElementChild } = scroll,
    unScroll = On(scroll, { scroll: onScroll });

  ob.observe(scroll);
  ob.observe(el);
  if (firstElementChild) {
    ob.observe(firstElementChild);
  }

  return () => {
    if (ticking) cancelAnimationFrame(ticking);
    unScroll();
    ob.disconnect();
    rmTimer();
    reset();
  };
};
