import { D, newEl, clearDom } from "../../../js/dom.js";

const ING = "Ing";

export default (demo_body) => {
  let sandbox = null,
    mod = null;

  const setMod = (val) => {
    mod = val;
    if (!sandbox) {
      sandbox = newEl("div");
      sandbox.className = "sandbox";
      demo_body.append(sandbox);
    }

    const { CSS, HTM, default: fn } = val || {},
      { classList } = sandbox;

    clearDom(sandbox);
    classList.add(ING);
    if (CSS) {
      CSS.forEach((css_fn) => css_fn(sandbox));
    }
    if (HTM) {
      const fragment = D.createRange().createContextualFragment(HTM);
      sandbox.appendChild(fragment);
    }
    if (fn) {
      fn(sandbox);
    }
    classList.remove(ING);
  };

  return [setMod, () => mod];
};
