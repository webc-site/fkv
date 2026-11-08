import "../../Blog.js";
import { newEl } from "../../../js/dom.js";
import cNew from "../cNew.js";

const BLOG = "blog";
let main_el, blog_el;

const initBlog = () => {
  if (!main_el) {
    main_el = newEl("main");
    blog_el = cNew("blog");
    blog_el.prefix = "/" + BLOG + "/";
    blog_el.mdUrl = (key, lang) =>
      (import.meta.env.DEV ? "/doc/" : GIT_DOC) + lang + "/blog/" + key + ".md";
    main_el.append(blog_el);
  }
  return main_el;
};

export default (url) => {
  if (url === BLOG || url.startsWith(BLOG + "/")) {
    const main = initBlog(),
      sub_key = url.startsWith(BLOG + "/") ? url.slice(5) : "";

    return [
      Promise.resolve(main),
      async (_, code) => {
        blog_el._ = (await import(`../_/blog/${code}.js`)).default;
        blog_el.load(sub_key);
      }
    ];
  }
};
