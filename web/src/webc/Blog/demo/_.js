import "../../Blog.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { T1, S1, T2, S2, T3, S3 } from "./I18N.js";

export default (root) => {
  const blog = root.querySelector("c-blog"),
    fLang = fetchLang(i18nUrl);

  if (blog) {
    blog.prefix = "/blog/";
    blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
    fLang((i18n) => {
      blog._ = {
        "2026/webc-v2-release": [i18n[T1], i18n[S1], "2026-07-27"],
        "2026/css-nesting-in-practice": [i18n[T2], i18n[S2], "2026-05-18"],
        "2025/building-zero-dependency-ui": [i18n[T3], i18n[S3], "2025-12-01"]
      };
    });
  }
};
