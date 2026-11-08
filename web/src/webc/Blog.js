import "./Btn.js";
import "./H1Txt.js";
import "./Md.js";
import "./O.js";
import i18nUrl, { READ_MORE } from "./Blog/I18N.js";
import "./Blog/_.styl";
import { cE, newEl, newLi, clearDom } from "../js/dom.js";
import headerNew from "../js/dom/header.js";
import { fetchLang } from "../js/fetchLang.js";
import { fTxt } from "../js/fetch.js";
import { LANG_CODE } from "../js/i18n.js";

const fLang = fetchLang(i18nUrl),
  renderList = (el, data, read_more_txt) => {
    clearDom(el);
    const year_map = new Map(),
      prefix = el.prefix;

    for (const [key, val_li] of Object.entries(data)) {
      if (!Array.isArray(val_li)) continue;
      const [title, summary, date, custom_btn_txt] = val_li,
        btn_txt = custom_btn_txt || read_more_txt,
        year = date ? date.slice(0, 4) : "",
        month_day = date ? date.slice(5) : "";

      let item_li = year_map.get(year);
      if (!item_li) year_map.set(year, (item_li = []));

      item_li.push([date, prefix + key, title, summary, month_day, btn_txt]);
    }

    const year_li = Array.from(year_map.keys()).sort((a, b) => b.localeCompare(a));

    for (const year of year_li) {
      const year_wrap = newEl("b"),
        item_li = year_map.get(year);

      if (year) {
        const h2 = newEl("h2");
        h2.textContent = year;
        year_wrap.append(h2);
      }

      item_li.sort((a, b) => (b[0] || "").localeCompare(a[0] || ""));

      for (const [, href, title, summary, month_day, btn_txt] of item_li) {
        const [item, h3, a, md, foot, btn, b_date] = newLi("b", "h3", "a", "c-md", "b", "a", "b");

        h3.className = "O";
        a.href = btn.href = href;
        a.textContent = title;

        if (summary) md.value = summary;

        btn.className = "Btn";
        btn.textContent = btn_txt;
        b_date.textContent = month_day;

        h3.append(a);
        foot.append(btn, b_date);
        item.append(h3, md, foot);
        year_wrap.append(item);
      }

      el.append(year_wrap);
    }
  },
  renderArt = async (el, key) => {
    clearDom(el);
    if (!key) return;

    const data = el._,
      info = data ? data[key] : null,
      title = info ? info[0] : key,
      date = info ? info[2] : "",
      [header, h1, b] = headerNew(),
      md_el = newEl("c-md");

    h1.textContent = title;
    b.textContent = date;

    el.append(header, md_el);

    const md_url_fn = el.mdUrl;
    if (!md_url_fn) return;

    const url = md_url_fn(key, LANG_CODE);

    try {
      let md = await fTxt(url);
      if (md) {
        const pos = md.indexOf("\n#");
        if (pos !== -1) md = md.slice(md.indexOf("\n", pos + 1) + 1);
        md_el.value = md.trimStart();
      }
    } catch (e) {
      md_el.value = e.message;
    }
  },
  render = async (el) => {
    if (el._key) {
      await renderArt(el, el._key);
    } else if (el._val && el._read_more) {
      renderList(el, el._val, el._read_more);
    }
    el.dispatchEvent(new Event("loaded", { bubbles: true }));
  };

cE(
  "blog",
  class extends HTMLElement {
    get mdUrl() {
      return this._md_url;
    }

    set mdUrl(fn) {
      this._md_url = fn;
    }

    get prefix() {
      return this._prefix;
    }

    set prefix(val) {
      if (this._prefix === val) return;
      this._prefix = val;
      render(this);
    }

    get _() {
      return this._val;
    }

    set _(val) {
      this._val = val;
      render(this);
    }

    load(key) {
      if (this._key === key) return;
      this._key = key;
      render(this);
    }

    connectedCallback() {
      this.$ = fLang((i18n) => {
        this._read_more = i18n[READ_MORE];
        render(this);
      });

      for (const k of ["prefix", "_"]) {
        if (Object.hasOwn(this, k)) {
          const val = this[k];
          delete this[k];
          this[k] = val;
        }
      }
    }

    disconnectedCallback() {
      if (this.$) {
        this.$();
        this.$ = null;
      }
    }
  }
);
