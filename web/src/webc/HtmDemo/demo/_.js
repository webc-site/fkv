import "../../MdCss.js";
export default (root) => {
  const el = root.querySelector("c-htm-demo");
  if (el) {
    el.gen = (type, text) => {
      const js = "console.log('hello world')",
        css = "p { color: blue }";
      return [text, css, js];
    };
  }
};
