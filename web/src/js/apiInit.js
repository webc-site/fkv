import { setApi, setFetch, setOnCaptcha, setOnErr } from "@1-/protoapi";
import Captcha from "../webc/Captcha.js";
import { LANG_CODE } from "./i18n.js";
import { toastErr } from "./Toast.js";

setApi(API);

setFetch((url, conf) => {
  conf.headers["accept-language"] = LANG_CODE;
  return fetch(url, conf);
});

setOnCaptcha(Captcha);

setOnErr((err) => {
  toastErr((el) => (el.textContent = err));
});
