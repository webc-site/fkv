import PKG from "./NPM.js";

const rmBefore = (str, sep) => {
    const pos = str.indexOf(sep);
    return pos === -1 ? str : str.slice(pos + sep.length);
  },
  // Compatible with / 兼容以下格式 :
  // git+https://github.com/webc.site/npm.git
  // git@github.com:webc.site/npm.git
  // ssh://git@ssh.github.com:443/webc.site/npm.git
  // https://github.com/webc.site/npm.git
  // to:
  // webc.site/npm
  USER_REPO = (() => {
    const url = PKG.repository.url,
      next = rmBefore(url, "://");
    return (next !== url ? rmBefore(next, "/") : rmBefore(url, ":")).replace(/\.git$/, "");
  })(),
  CDN_HOST = "cdn.jsdmirror.com",
  gitCdn = (host) => "//" + host + "/gh/" + USER_REPO;

export const BRANCH_PATH = "/main/",
  GIT_CDN = gitCdn(CDN_HOST) + "@",
  svg = (lang) =>
    // npmjs.com not support  '//'
    "https:" + GIT_CDN + "main/doc/.svg/" + lang + ".svg",
  GIT_URL = "//github.com/" + USER_REPO + "/",
  GIT_CODE_URL = GIT_URL + "blob" + BRANCH_PATH,
  readme = (lang) => GIT_CODE_URL + "doc/" + lang + "/doc/README.md";
