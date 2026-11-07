import "../../Md.js";

const DOC = {
  zh: {
    README: "WebC.site",
    i18n: "国际化",
    use: "使用组件",
    npm: "发布 NPM 包",
    dist: "上线展示站",
    bench: "基准评测"
  },
  en: {
    README: "WebC.site",
    i18n: "Internationalization",
    use: "Use Component",
    npm: "Publish NPM Package",
    dist: "Deploy Site",
    bench: "Benchmark"
  }
};

export default (root) => {
  const toc = root.querySelector("c-toc"),
    md = root.querySelector("c-md");

  if (!toc) return;

  if (md) toc.target = md;
  toc.prefix = "doc";
  toc.mdUrl = (key, lang) =>
    (import.meta.env.DEV ? "/doc/" : GIT_DOC) + lang + "/doc/" + key + ".md";
  toc.li = (lang) => DOC[lang] || DOC.en;

  const first_key = Object.keys(DOC.en)[0];
  if (first_key) toc.load(first_key);
};
