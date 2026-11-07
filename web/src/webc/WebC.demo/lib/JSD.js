export const PKG_VER = NPM_NAME + "@" + NPM_VERSION,
  jsdUrl = (host) => "//" + host + "/npm/" + PKG_VER + "/",
  HOST_LI = ["cdn.jsdmirror.com", "cdn.jsdelivr.net", "fastly.jsdelivr.net", "gcore.jsdelivr.net"];

export default jsdUrl(HOST_LI[0]);
