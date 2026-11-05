import { newEl } from "../../../js/dom.js";
import headerNew from "../../../js/dom/header.js";
import { BRIEF } from "../I18N.js";

let res_li;

const page = () => {
  const el = newEl("main"),
    [header, h1, b] = headerNew();

  el.className = "index";
  el.append(header);
  res_li = [
    Promise.resolve(el),
    (i18n) => {
      h1.textContent = "fkv";
      b.textContent = i18n[BRIEF];
    }
  ];
  return res_li;
};

export default (url) => {
  if (!url) {
    return res_li || page();
  }
};
