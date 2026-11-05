const make = (host, name, version) => "//" + host + "/npm/" + name + "@" + version + "/";

export default {
  "cdn.jsdmirror.com": make,
  "cdn.jsdelivr.net": make,
  "fastly.jsdelivr.net": make,
  "gcore.jsdelivr.net": make
};
