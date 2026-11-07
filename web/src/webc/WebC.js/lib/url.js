export const docUrl = (rel, lang) =>
    import.meta.env.DEV
      ? "/doc/" + lang + "/js/" + rel + ".md"
      : GIT_DOC + lang + GIT_JS_README_I18N_DIR + rel + GIT_JS_README_MD,
  srcUrl = (rel) =>
    import.meta.env.DEV ? "/src/js/" + rel + ".js" : GIT_JS_SRC_PREFIX + rel + GIT_JS_SRC_SUFFIX;
