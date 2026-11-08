import GIT_REV from "./GIT_REV.js";
import { GIT_CDN as raw_cdn } from "./URL.js";

export const GIT_CDN = raw_cdn + GIT_REV + "/",
  GIT_DOC = GIT_CDN + "doc/",
  GIT_WEBC_README_PREFIX = GIT_CDN + "src/webc/",
  GIT_WEBC_README_I18N_DIR = "/i18n/",
  GIT_WEBC_README_MD = "README.md",
  GIT_JS_README_I18N_DIR = "/js/",
  GIT_JS_README_MD = ".md",
  GIT_JS_SRC_PREFIX = GIT_CDN + "src/js/",
  GIT_JS_SRC_SUFFIX = ".js";
