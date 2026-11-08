export default (url, lang) =>
  import.meta.env.DEV
    ? "/src/webc/" + url + "/i18n/" + lang + "/README.md"
    : GIT_WEBC_README_PREFIX + url + GIT_WEBC_README_I18N_DIR + lang + "/" + GIT_WEBC_README_MD;
