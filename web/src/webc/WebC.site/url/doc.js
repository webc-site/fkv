import "../../Toc.js";
import "../../Md.js";
import { newEl } from "../../../js/dom.js";
import cNew from "../cNew.js";

const DOC = "doc";
let main_el, toc_el, md_el;

const initDoc = () => {
  if (!main_el) {
    main_el = newEl("main");
    main_el.className = DOC;
    toc_el = cNew("toc");
    md_el = cNew("md");
    main_el.append(toc_el, md_el);
    toc_el.target = md_el;
    toc_el.prefix = DOC;
    toc_el.mdUrl = (key, lang) =>
      (import.meta.env.DEV ? "/" + DOC + "/" : GIT_DOC) + lang + "/doc/" + key + ".md";
  }
  return main_el;
};

export default (url) => {
  if (url === DOC || url.startsWith(DOC + "/")) {
    const main = initDoc(),
      sub_key = url === DOC || url === DOC + "/" ? "" : url.slice(DOC.length + 1);

    return [
      Promise.resolve(main),
      async (_, code) => {
        const mod = await import(`../_/doc/${code}.js`),
          dict = mod.default;
        toc_el.li = () => dict;
        const key = sub_key || Object.keys(dict)[0];
        if (key) {
          toc_el.load(key);
        }
      }
    ];
  }
};
