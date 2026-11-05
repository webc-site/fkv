import "./Scroll.js";
import "./Ing.js";
import "./Side.js";
import "./Btn.js";
import "./Toc/_.styl";
import { cE, newEl, newLi } from "../js/dom.js";
import { fTxt } from "../js/fetch.js";
import hashActive from "../js/hashActive.js";
import { LANG_CODE, onLang } from "../js/i18n.js";
import navScroll from "../js/navScroll.js";
import { On } from "../js/On.js";
import { goto } from "../js/route.js";

const TOC = "toc",
  C_A = "A",
  C_ING = "Ing",
  C_SUB = "sub",
  C_SIDE = "c-side",
  C_VS = "c-vs",
  C_IN = "In",
  TAG_H1 = "H1",
  PREFIX_H = "H",
  HASH = "#",
  HREF = "href",
  A_DATA_KEY = "a[data-key]",
  QUERY_TARGET = "c-md,main,article",
  QUERY_H = "h1,h2,h3,h4,h5,h6",
  README = "README",
  UN_HASH = "_un_hash",
  UN_H1_NAV = "_un_h1_nav",
  toHash = (id) => HASH + id,
  mdUrlByKey = (self, key) => self.mdUrl(key, LANG_CODE),
  urlByKey = (self, key) => {
    let p = self._prefix || "";
    if (p.endsWith("/")) p = p.slice(0, -1);
    return key === README ? p : p ? p + "/" + key : key;
  },
  keyA = (self, key) => vsEl(self).querySelector('a[data-key="' + key + '"]'),
  activeA = (self) =>
    self._key ? keyA(self, self._key) : vsEl(self).querySelector("a." + C_A + "[data-key]"),
  unProp = (self, k) => {
    if (self[k]) {
      self[k]();
      self[k] = null;
    }
  },
  unReset = (self) => {
    unProp(self, UN_HASH);
    unProp(self, UN_H1_NAV);
  },
  rmSub = (vs) => vs.querySelectorAll("." + C_SUB).forEach((el) => el.remove()),
  bindH1Nav = (self, h1) => {
    unProp(self, UN_H1_NAV);
    if (h1) self[UN_H1_NAV] = navScroll(h1);
  },
  hrefRefresh = (self) =>
    vsEl(self)
      .querySelectorAll(A_DATA_KEY)
      .forEach((a) => {
        a.href = urlByKey(self, a.dataset.key);
      }),
  isOpen = (a) =>
    Boolean(
      a &&
      (a.classList.contains(C_A) ||
        (a.nextElementSibling && a.nextElementSibling.classList.contains(C_SUB)))
    ),
  btnEl = (self) => {
    let wrap = self._btn_wrap;
    if (!wrap) {
      const [wrap_el, btn, ico] = newLi("div", "button", "i");
      wrap = wrap_el;
      wrap.className = "TocBtn";
      btn.className = "BtnC";
      btn.type = "button";
      ico.className = "Ico";
      btn.append(ico);
      btn.onclick = (e) => {
        e.stopPropagation();
        const side = sideEl(self);
        if (side.open) side.open();
      };
      wrap.append(btn);
      self._btn_wrap = wrap;
    }
    return wrap;
  },
  sideEl = (self) => {
    let side = self.querySelector(C_SIDE);
    if (!side) {
      side = newEl(C_SIDE);
      side.className = "right";
      self.append(side);
    }
    return side;
  },
  vsEl = (self) => {
    const side = sideEl(self);
    let vs = side.querySelector(C_VS);
    if (!vs) {
      let in_el = side.querySelector("." + C_IN);
      if (!in_el) {
        [in_el, vs] = newLi("div", C_VS);
        in_el.className = C_IN;
        side.append(in_el);
      } else {
        vs = newEl(C_VS);
        in_el.append(vs);
      }
    }
    return vs;
  },
  targetEl = (self) =>
    self._target ||
    self.getRootNode().querySelector(QUERY_TARGET) ||
    document.querySelector(QUERY_TARGET),
  subRender = (self, a) => {
    unReset(self);
    rmSub(vsEl(self));

    const target_el = targetEl(self);
    if (!target_el) return;

    const h_li = Array.from(target_el.querySelectorAll(QUERY_H));
    if (!h_li.length) {
      self[UN_HASH] = hashActive([a]);
      return;
    }

    let idx = 0;
    const base_href = a.href.split(HASH)[0];

    if (h_li[0].tagName === TAG_H1) {
      const first_h1 = h_li.shift(),
        id = PREFIX_H + ++idx,
        btn = btnEl(self);
      first_h1.id = id;
      a.href = base_href + toHash(id);

      if (!first_h1.contains(btn)) {
        first_h1.append(btn);
      }
      bindH1Nav(self, btn);
    } else {
      if (!target_el.id) target_el.id = PREFIX_H + ++idx;
      a.href = base_href + toHash(target_el.id);
    }

    if (h_li.length) {
      const sub = newEl("div");
      sub.className = C_SUB;

      h_li.forEach((h) => {
        const text = h.textContent.trim();
        if (!text) return;

        const id = PREFIX_H + ++idx,
          sub_a = newEl("a");

        h.id = id;
        if (h.tagName !== TAG_H1 && h.parentElement.tagName !== "A") {
          const ha = newEl("a");
          ha.href = toHash(id);
          h.replaceWith(ha);
          ha.append(h);
        }
        sub_a.className = "h" + h.tagName[1].toLowerCase();
        sub_a.href = toHash(id);
        sub_a.textContent = text;
        sub.append(sub_a);
      });

      if (sub.children.length) {
        a.after(sub);
        self[UN_HASH] = hashActive([a, ...sub.children]);
        return;
      }
    }

    self[UN_HASH] = hashActive([a]);
  },
  render = (self, data) => {
    const vs = vsEl(self);
    vs.innerHTML = "";
    if (!data) return;

    const real_data = data(LANG_CODE);
    if (!real_data) return;

    Object.entries(real_data).forEach(([key, title]) => {
      const a = newEl("a");
      a.dataset.key = key;
      a.href = urlByKey(self, key);
      a.textContent = title;
      vs.append(a);
    });
  },
  keyLoad = async (self, a, force) => {
    if (!a || a.classList.contains(C_ING) || (!force && isOpen(a))) return;
    const { key } = a.dataset;
    if (!key) return;

    self._key = key;

    const url = mdUrlByKey(self, key),
      target_el = targetEl(self),
      vs = vsEl(self);

    unReset(self);

    vs.querySelectorAll("a." + C_A).forEach((el) => el.classList.remove(C_A));
    rmSub(vs);

    a.classList.add(C_A, C_ING);
    if (target_el) {
      target_el.classList.add(C_ING);
      if ("value" in target_el) target_el.value = "";
      target_el.innerHTML = "";
    }

    let md;
    try {
      md = await fTxt(url);
    } finally {
      a.classList.remove(C_ING);
      if (target_el) target_el.classList.remove(C_ING);
    }

    if (target_el) {
      if ("value" in target_el) target_el.value = md;
      else target_el.innerHTML = md;
    }

    setTimeout(() => {
      if (a.classList.contains(C_A)) {
        subRender(self, a);
        docNavRender(self);
      }
    }, 0);
  },
  docNavRender = (self) => {
    const target_el = targetEl(self);
    if (!target_el) return;

    let nav = target_el.querySelector(".DocNav");
    if (nav) nav.remove();

    if (!self._li || !self._key) return;
    const dict = self._li(LANG_CODE);
    if (!dict) return;

    const key_li = Object.keys(dict),
      idx = key_li.indexOf(self._key);
    if (idx < 0) return;

    const prev_key = key_li[idx - 1],
      next_key = key_li[idx + 1];
    if (!prev_key && !next_key) return;

    nav = newEl("b");
    nav.className = "DocNav";

    [
      ["prev", prev_key],
      ["next", next_key]
    ].forEach(([cls, key]) => {
      if (key) {
        const a = newEl("a"),
          span = newEl("span"),
          ico = newEl("i");
        a.className = cls;
        a.href = urlByKey(self, key);
        a.dataset.key = key;
        span.textContent = dict[key];
        ico.className = "Ico";
        if (cls === "prev") a.append(ico, span);
        else a.append(span, ico);
        nav.append(a);
      }
    });

    nav.onclick = (e) => {
      const a = e.target.closest("a[data-key]");
      if (!a) return;
      e.preventDefault();
      const { key } = a.dataset;
      if (key) {
        goto(urlByKey(self, key));
        self.load(key);
        window.scrollTo({ top: 0, behavior: "smooth" });
      }
    };

    target_el.append(nav);
  };

cE(
  TOC,
  class extends HTMLElement {
    get li() {
      return this._li;
    }

    set li(val) {
      if (this._li === val) return;
      this._li = val;
      render(this, val);
      if (this._key) {
        const a = keyA(this, this._key);
        if (a && !isOpen(a)) keyLoad(this, a);
      }
    }

    get prefix() {
      return this._prefix || "";
    }

    set prefix(val) {
      if (this._prefix === val) return;
      this._prefix = val;
      hrefRefresh(this);
    }

    get mdUrl() {
      return this._md_url;
    }

    set mdUrl(fn) {
      this._md_url = fn;
    }

    get target() {
      return this._target;
    }

    set target(val) {
      this._target = val;
    }

    connectedCallback() {
      const side = sideEl(this),
        un_click = On(this, {
          click: (e) => {
            const a = e.target.closest("a");
            if (!a || !this.contains(a)) return;

            if (side.classList.contains("open")) {
              side.close();
            }

            const { key } = a.dataset;
            if (key) {
              e.preventDefault();
              if (isOpen(a)) {
                const href = a.getAttribute(HREF);
                if (href && href.includes(HASH)) {
                  const id = href.split(HASH).pop(),
                    hash = toHash(id);
                  if (location.hash === hash) {
                    const root = this.getRootNode(),
                      doc = root.getElementById ? root : document,
                      el = doc.getElementById(id);
                    if (el) el.scrollIntoView({ behavior: "smooth", block: "start" });
                  } else {
                    location.hash = hash;
                  }
                }
                return;
              }
              goto(urlByKey(this, key));
              keyLoad(this, a);
            }
          }
        }),
        un_lang = onLang(() => {
          if (this._li) render(this, this._li);
          if (this._key) {
            const a = keyA(this, this._key);
            if (a) keyLoad(this, a, 1);
          }
        });

      this.$ = () => {
        unReset(this);
        un_click();
        un_lang();
      };
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
        this.$ = null;
      }
    }

    load(key) {
      this._key = key;
      const a = keyA(this, key);
      if (a && !isOpen(a)) keyLoad(this, a);
    }

    sync() {
      const a = activeA(this);
      if (a) subRender(this, a);
    }
  }
);
