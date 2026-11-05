import "./Scroll/_.styl";
import { On } from "../js/On.js";
import { D, cE, newEl } from "../js/dom.js";

(() => {
  const supports_none = CSS.supports("scrollbar-width", "none"),
    { round, max, min } = Math,
    CLIENT = "client",
    SCROLL = "scroll",
    DRAG = "drag",
    PX = "px",
    mk = (tag, part, ...kids) => {
      const e = newEl(tag);
      if (part) e.setAttribute("part", part);
      e.append(...kids);
      return e;
    },
    init = (size, pos, axis) => {
      // 若不支持 scrollbar-width: none，直接返回
      if (!supports_none) return;
      const style_size = size.toLowerCase(),
        style_pos = pos.toLowerCase(),
        client_size = CLIENT + size,
        scroll_size = SCROLL + size,
        scroll_pos = SCROLL + pos,
        client_pos = CLIENT + axis;
      return (ct) => {
        let timer_bar,
          ptr_unbind,
          timer_resize,
          pre_st = -1;
        const inner = ct.firstElementChild,
          si = mk("i", "si"),
          bar = mk("b", "bar", si),
          geo = (sih = si[client_size]) => {
            const ch = ct[client_size],
              sh = inner[scroll_size];
            return [sh - ch, max(1, ch - sih - 6), sih, ch, sh];
          },
          up = (h) => {
            const [ds, db, , ch, sh] = geo(h);
            if (ds <= 0) return;
            const parent = ct.parentNode;
            if (!bar.parentNode) {
              parent.appendChild(bar);
              si.style[style_size] = max(16, round((ch * ch) / sh)) + PX;
            }
            const st = max(0, min(ct[scroll_pos], ds));
            if (pre_st != -1 && pre_st != st) {
              bar.style.opacity = 1;
              clearTimeout(timer_bar);
              timer_bar = setTimeout(() => (bar.style.opacity = 0), 1e3);
            }
            pre_st = st;
            si.style[style_pos] = 3 + round((db * st) / ds) + PX;
          },
          onDown = (e) => {
            if (ptr_unbind) return;
            const bd = D.body;
            bd.setPointerCapture(e.pointerId);
            bd.classList.add(DRAG);
            bar.part.add(DRAG);
            let pre = e[client_pos];
            const detach = () => {
                bd.classList.remove(DRAG);
                bar.part.remove(DRAG);
                un_ptr();
                ptr_unbind = null;
              },
              un_ptr = On(bd, {
                pointermove: (evt) => {
                  const [ds, db] = geo();
                  ct[scroll_pos] += round((ds * (evt[client_pos] - pre)) / db);
                  pre = evt[client_pos];
                },
                pointerup: detach,
                lostpointercapture: detach
              });
            ptr_unbind = detach;
          },
          onClick = (e) => {
            const rect = bar.getBoundingClientRect(),
              top = rect[style_pos],
              [ds, db, sih] = geo();
            ct[scroll_pos] = round(ds * max(min((e[client_pos] - top - 3 - sih / 2) / db, 1), 0));
            onDown(e);
          },
          ob = new ResizeObserver(() => {
            clearTimeout(timer_resize);
            timer_resize = setTimeout(() => {
              const [, , , ch, sh] = geo(),
                is_turn = ch < sh;
              if (is_turn) {
                const parent = ct.parentNode;
                if (bar.parentNode != parent) parent.appendChild(bar);
                const h = max(16, round((ch * ch) / sh));
                si.style[style_size] = h + PX;
                up(h);
              } else if (bar.parentNode) {
                bar.remove();
              }
            }, 200);
          }),
          unbind = [
            On(bar, { pointerdown: onClick }),
            On(si, {
              pointerdown: (e) => {
                e.stopPropagation();
                onDown(e);
              }
            }),
            On(ct, { scroll: up.bind(null, undefined) })
          ],
          destroy = () => {
            clearTimeout(timer_bar);
            clearTimeout(timer_resize);
            unbind.forEach((f) => f());
            if (ptr_unbind) ptr_unbind();
            ob.disconnect();
            if (bar.parentNode) bar.remove();
          };
        bar.style.opacity = 0;
        ob.observe(ct);
        ob.observe(inner);
        return destroy;
      };
    },
    UNBIND = Symbol();
  [
    ["vs", "Height", "Top", "Y"],
    ["hs", "Width", "Left", "X"]
  ].map(([prefix, size, pos, axis]) => {
    const setup = init(size, pos, axis);
    cE(
      prefix,
      class extends HTMLElement {
        connectedCallback() {
          if (this.shadowRoot) return;
          const shadow = this.attachShadow({ mode: "open" }),
            wrapper = mk("b", SCROLL, mk("b", "i", mk("slot")));
          shadow.append(wrapper);
          // 若支持 scrollbar-width: none，初始化自定义滚动条
          if (supports_none) {
            wrapper.style.scrollbarWidth = "none";
            this[UNBIND] = setup(wrapper);
          }
        }
        disconnectedCallback() {
          if (this[UNBIND]) this[UNBIND]();
        }
      }
    );
  });
})();
