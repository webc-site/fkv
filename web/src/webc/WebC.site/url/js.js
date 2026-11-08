import cNew from "../cNew.js";

export default (url) => {
  if (url.startsWith("js/")) {
    const el = cNew("webc-js"),
      rel = url.slice(3);
    el.setAttribute("rel", rel);
    return [Promise.resolve(el), () => {}];
  }
};
