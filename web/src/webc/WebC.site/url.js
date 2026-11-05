import { route } from "../../js/route.js";
import fLang from "./fLang.js";
import sandbox from "./url/sandbox.js";
import index from "./url/index.js";
import js from "./url/js.js";
import doc from "./url/doc.js";
import blog from "./url/blog.js";

const FN_LI = [index, doc, blog, js, sandbox];

export default (onLoading, onLoaded, onErr) => {
  let unBind = () => {};

  const unRoute = route(async (url) => {
    onLoading(url);
    for (const fn of FN_LI) {
      const p_li = fn(url);
      if (p_li) {
        unBind();
        const unBindLang = fLang(p_li[1]);
        unBind = () => {
          unBindLang();
          if (p_li[2]) p_li[2]();
        };
        try {
          onLoaded(url, await p_li[0]);
        } catch (e) {
          onErr(url, [e.message, e.stack]);
        }
        break;
      }
    }
  });

  return () => {
    unRoute();
    unBind();
  };
};
