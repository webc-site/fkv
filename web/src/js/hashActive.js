import { On } from "./On.js";

const HASH = "#";

export default (link_li, cls = "A") => {
  let act_link = null,
    clicking = false;

  const [first] = link_li,
    target_li = [],
    link_m = new Map(),
    intersect_m = new Map(),
    menuB = () => (first ? first.getBoundingClientRect().bottom : 0),
    idByHref = (href) => {
      const pos = href ? href.indexOf(HASH) : -1;
      return pos !== -1 ? href.slice(pos + 1) : "";
    },
    find = (hash) => {
      if (!hash) return null;
      const id = hash.startsWith(HASH) ? hash.slice(1) : hash,
        decoded_id = decodeURIComponent(id);
      return target_li.find((t) => t.id === id || t.id === decoded_id);
    },
    toggle = (active) => {
      if (active && active !== act_link) {
        link_li.forEach((link) => link.classList.toggle(cls, link === active));
        act_link = active;
      }
    },
    update = () => {
      if (clicking) return;

      const { innerHeight } = window,
        is_b = window.scrollY + innerHeight >= document.documentElement.scrollHeight - 10,
        menu_b = menuB();
      let cur = null,
        min_t = Infinity;

      target_li.forEach((target) => {
        if (intersect_m.get(target)) {
          const { top, bottom } = target.getBoundingClientRect();
          if (bottom > menu_b && (is_b || top < innerHeight * 0.5) && top < min_t) {
            min_t = top;
            cur = target;
          }
        }
      });

      if (!cur && target_li.length && !act_link) {
        const { hash } = location;
        cur = (hash && find(hash)) || target_li[0];
      }

      if (cur) toggle(link_m.get(cur));
    },
    obs = new IntersectionObserver(
      (entry_li) => {
        entry_li.forEach(({ target, isIntersecting }) => intersect_m.set(target, isIntersecting));
        update();
      },
      { rootMargin: "-" + menuB() + "px 0px -10% 0px" }
    ),
    go = () => {
      const { hash } = location,
        target = hash && find(hash);
      if (target) {
        clicking = true;
        toggle(link_m.get(target));

        const menu_b = menuB(),
          done = () => {
            clicking = false;
            off_s();
            clearTimeout(timer);
          },
          off_s = On(window, { scrollend: done }),
          timer = setTimeout(done, 1000);

        target.style.scrollMarginTop = menu_b + "px";
        target.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    },
    off = On(window, { hashchange: go });

  link_li.forEach((link) => {
    const id = idByHref(link.getAttribute("href"));
    if (id) {
      const root = link.getRootNode(),
        target = root.getElementById
          ? root.getElementById(id)
          : root.querySelector('[id="' + id + '"]');
      if (target) {
        target_li.push(target);
        link_m.set(target, link);
      }
    }
  });

  target_li.forEach((target) => obs.observe(target));
  update();

  return () => {
    obs.disconnect();
    off();
  };
};
