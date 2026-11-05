export const D = document,
  B = D.body,
  css = (txt, root = D) => {
    const sheet = new CSSStyleSheet(),
      { adoptedStyleSheets: style } = root;
    sheet.replace(txt);
    if (!style.includes(sheet)) {
      style.push(sheet);
    }
    return sheet;
  },
  newEl = D.createElement.bind(D),
  newLi = (...tag_li) => tag_li.map(newEl),
  clearDom = (dom) => dom.replaceChildren(),
  cE = (name, cls) => {
    name = "c-" + name;
    if (import.meta.env.DEV) {
      if (customElements.get(name)) {
        console.log("⚠️ " + name + " exist");
        return;
      }
    }
    customElements.define(name, cls);
  };
