import { On } from "../../js/On.js";

const KEEP_LI = ["", "blog"],
  pos_map = new Map(),
  scrollEl = (el) => el.shadowRoot.firstElementChild;

export default (scroll, urlGet) => {
  let is_loading = false;

  const target = scrollEl(scroll),
    save = (url) => {
      if (!is_loading && KEEP_LI.includes(url)) {
        pos_map.set(url, target.scrollTop);
      }
    },
    restore = (url) => {
      is_loading = false;
      const top = pos_map.get(url);
      target.scrollTop = KEEP_LI.includes(url) ? top || 0 : 0;
    },
    leave = (old_url) => {
      save(old_url);
      is_loading = true;
    },
    un_scroll_evt = On(target, {
      scroll: () => save(urlGet())
    }),
    un_loaded_evt = On(scroll, {
      loaded: () => restore(urlGet())
    }),
    un_scroll = () => {
      un_scroll_evt();
      un_loaded_evt();
    };

  return [leave, restore, un_scroll];
};
