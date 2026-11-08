import CODE from "./i18n/CODE.js";
import { bc } from "./bc.js";

export let LANG, LANG_CODE;

const HOOK = new Set(),
  [post, on] = bc("lang"),
  change = (id) => {
    LANG = id;
    LANG_CODE = CODE[id];
    HOOK.forEach((f) => f(LANG_CODE, LANG));
  };

export const LANG_LI = [],
  onLang = (func) => {
    if (LANG !== undefined) func(LANG_CODE, LANG);
    HOOK.add(func);
    return () => {
      HOOK.delete(func);
    };
  },
  langSet = (id) => {
    if (id !== LANG) {
      post(id);
      change(id);
    }
  };

on((id) => {
  if (id !== LANG) change(id);
});
