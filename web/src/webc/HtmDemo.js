import "./SelectBtn.js";
import i18nUrl, { RUN } from "./HtmDemo/I18N.js";
import CodeSandbox from "./HtmDemo/site/CodeSandbox.js";
import JSFiddle from "./HtmDemo/site/JSFiddle.js";
import CodePen from "./HtmDemo/site/CodePen.js";
import "./HtmDemo/_.styl";
import { cE, newEl } from "../js/dom.js";
import { On } from "../js/On.js";
import { fetchLang } from "../js/fetchLang.js";

const HTML = 1,
  JS = 2,
  fLang = fetchLang(i18nUrl),
  KEY = "webc-demo-plat",
  PLAT_MAP = {
    CodeSandbox,
    JSFiddle,
    CodePen
  };

cE(
  "htm-demo",
  class extends HTMLElement {
    set gen(val) {
      this._gen = val;
      if (this._connected) this._init();
    }

    get gen() {
      return this._gen;
    }

    connectedCallback() {
      this._connected = true;
      queueMicrotask(() => this._init());
    }

    disconnectedCallback() {
      this._connected = false;
      if (this.$) {
        this.$();
        this.$ = null;
      }
    }

    _init() {
      const self = this;
      if (!self._connected || !self.gen || self.$) return;

      const [js_el, html_el] = ["js", "html"].map((x) => self.querySelector(".language-" + x)),
        type = js_el ? JS : HTML,
        text = js_el ? js_el.textContent.trim() : (html_el || self).textContent.trim(),
        [head, body, css, js] = self.gen(type, text),
        bar = newEl("c-select-btn"),
        btn = newEl("button"),
        sel = newEl("c-select"),
        platform_li = ["CodeSandbox", "JSFiddle", "CodePen"];

      bar.setAttribute("key", KEY);
      bar.setAttribute("value", "CodeSandbox");

      platform_li.forEach((name) => {
        const opt = newEl("option");
        opt.value = opt.textContent = name;
        sel.append(opt);
      });

      bar.append(sel, btn);
      self.appendChild(bar);

      const submit = (e) => {
          const plat = e.value;
          PLAT_MAP[plat](head, body, css, js);
        },
        unBtn = On(bar, { submit }),
        unLang = fLang((i18n) => {
          btn.textContent = i18n[RUN];
        });

      self.$ = () => {
        unBtn();
        unLang();
      };
    }
  }
);
