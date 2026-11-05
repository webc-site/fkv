import I18N_URL, { NAV } from "./WebC.demo/I18N.js";
import "./H1Txt.js";
import "./Nav.js";
import "./HtmDemo.js";
import "./Edit.js";
import "./Code.js";
import tabNew from "./WebC.demo/lib/tab.js";
import mdInit from "./WebC.demo/lib/md.js";
import sandboxInit from "./WebC.demo/lib/sandbox.js";
import setMdInit from "./WebC.demo/lib/setMd.js";
import "./WebC.demo/_.styl";
import { newEl, newLi } from "../js/dom.js";
import headerNew from "../js/dom/header.js";
import { fetchLang } from "../js/fetchLang.js";
import hashActive from "../js/hashActive.js";

const fLang = fetchLang(I18N_URL);

export default (url) => {
  let unHashActive;

  const root = newEl("c-webc-demo"),
    [header, h1, b] = headerNew(),
    [
      nav,
      menu,
      doc_h2,
      demo_div,
      use_div,
      demo_h2,
      demo_body,
      left_div,
      right_div,
      use_sec,
      doc_sec
    ] = newLi("c-nav", "b", "h2", "b", "main", "h2", "b", "b", "b", "b", "b"),
    main_div = newEl("b"),
    [tab_el, tabRender, tabUpdateLang] = tabNew(),
    [titleRender, mdRender, doc_md] = mdInit(h1, b, doc_h2, url),
    [setMod, modGet] = sandboxInit(demo_body),
    init_promise = (async () => {
      const mod = await (import.meta.env.DEV
        ? import(`./${url}/demo/_.js`)
        : import(`../_/demo/${url}.js`));
      setMod(mod);
      tabRender(url);
      return root;
    })(),
    [setMd, setDocTitle, doc_div] = setMdInit(
      init_promise,
      modGet,
      titleRender,
      mdRender,
      url,
      doc_md
    );

  [doc_h2, demo_h2].forEach((h) => h.classList.add("O"));
  menu.className = "NavALi";
  nav.append(menu);

  [
    [demo_div, "demo", [demo_h2, demo_body]],
    [use_div, "use", [tab_el]]
  ].forEach(([el, id, child_li]) => {
    el.id = id;
    el.append(...child_li);
  });

  left_div.className = "L";
  use_sec.append(use_div);
  doc_sec.append(doc_h2, doc_div);
  left_div.append(use_sec, demo_div);

  right_div.className = "R";
  right_div.append(doc_sec);

  main_div.className = "M";
  main_div.append(left_div, right_div);

  root.append(header, nav, main_div);

  const unLang = fLang((i18n, code) => {
      const nav_dict = i18n[NAV];
      setDocTitle(nav_dict.doc, code);

      const [span, code_btn] = newLi("span", "c-code");
      span.textContent = nav_dict.demo;
      code_btn.setAttribute("href", GIT_SRC + "webc/" + url);
      demo_h2.replaceChildren(span, code_btn);

      tabUpdateLang(i18n);

      menu.replaceChildren(
        ...["use", "demo", "doc"].map((key) => {
          const link = newEl("a");
          link.href = "#" + key;
          link.className = "Card";
          link.textContent = nav_dict[key];
          return link;
        })
      );

      if (unHashActive) unHashActive();
      unHashActive = hashActive(menu.querySelectorAll("a"));
    }),
    unMount = () => {
      unLang();
      if (unHashActive) unHashActive();
    },
    updateLang = (_, lang) => {
      setMd(lang);
    };

  return [init_promise, updateLang, unMount, setMd, root];
};
