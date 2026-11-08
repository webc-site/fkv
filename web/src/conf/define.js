import PKG from "./NPM.js";

export default async (is_dev) => {
  const { GIT_URL, BRANCH_PATH } = await import("./URL.js"),
    GIT_TREE = GIT_URL + "tree" + BRANCH_PATH,
    r = {
      NPM_NAME: PKG.name,
      NPM_VERSION: PKG.version,
      DOC_EDIT: GIT_URL + "edit" + BRANCH_PATH,
      GIT_SRC: GIT_TREE + "src/",
      GIT_SRC_BLOB: GIT_URL + "blob" + BRANCH_PATH + "src/",
      API_CAPTCHA: "https://captcha.webc.pub",
      API: "http://127.0.0.1:3003"
    };

  if (!is_dev) {
    Object.assign(r, await import("./GIT_CDN.js"));
  }
  return r;
};
