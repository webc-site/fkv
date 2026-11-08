import authGetApi from "@webc.site/api/auth/get.js";
import exitApi from "@webc.site/api/auth/user/exit.js";
import bidUserRm from "@webc.site/api/auth/user/bidRm.js";
import touchApi from "@webc.site/api/auth/user/touch.js";
import mailSigninApi from "@webc.site/api/auth/mail/signin.js";
import userNewByMailApi from "@webc.site/api/auth/mail/userNew.js";
import mailChangeApi from "@webc.site/api/auth/mail/change.js";
import { bc } from "./bc.js";
import { toastErr } from "./Toast.js";
import sleep from "@3-/sleep";

export let UID = 0;

let AUTH_TYPE_LI,
  USER_LI = [];

const { promise: AUTH_PROMISE, resolve: resolveAuth } = Promise.withResolvers(),
  HOOK = new Set(),
  [post, on] = bc("auth"),
  calcUid = () => {
    const u = USER_LI.find((item) => item[2]);
    return u ? u[0] : 0;
  },
  change = (user_li, type_li) => {
    if (user_li) USER_LI = user_li;
    if (type_li) AUTH_TYPE_LI = type_li;
    UID = calcUid();
    HOOK.forEach((f) => f(UID, USER_LI, AUTH_TYPE_LI));
  },
  sync = (user_li, type_li) => {
    change(user_li, type_li);
    post([USER_LI, AUTH_TYPE_LI]);
  };

on(([user_li, type_li]) => {
  change(user_li, type_li);
});

(async () => {
  for (;;) {
    try {
      const [type_li, user_li] = (await authGetApi()) || [];
      change(user_li || [], type_li || []);
      resolveAuth([AUTH_TYPE_LI, USER_LI]);
      break;
    } catch (err) {
      toastErr((el) => (el.textContent = err.message || err));
      await sleep(1000);
    }
  }
})();

export const userLi = () => USER_LI,
  authTypeLi = () => AUTH_TYPE_LI,
  onAuth = (func) => {
    if (AUTH_TYPE_LI !== undefined) func(UID, USER_LI, AUTH_TYPE_LI);
    HOOK.add(func);
    return () => {
      HOOK.delete(func);
    };
  },
  authGet = () => AUTH_PROMISE,
  authRefresh = async () => {
    const [type_li, user_li] = (await authGetApi()) || [];
    sync(user_li || [], type_li || []);
    return [AUTH_TYPE_LI, USER_LI];
  },
  touch = async (id) => {
    const res = await touchApi(id);
    if (res && res[0]) {
      const pos = USER_LI.findIndex(([uid]) => uid === id);
      if (pos >= 0) {
        USER_LI[pos][2] = true;
        if (pos > 0) {
          const [item] = USER_LI.splice(pos, 1);
          USER_LI.unshift(item);
        }
      }
      sync(USER_LI);
    }
    return res;
  },
  exit = async (id) => {
    const res = await exitApi(id),
      u = USER_LI.find(([uid]) => uid === id);
    if (u) {
      u[2] = false;
      const pos = USER_LI.findIndex(([, , is_login]) => is_login);
      if (pos > 0) {
        const [item] = USER_LI.splice(pos, 1);
        USER_LI.unshift(item);
      }
      sync(USER_LI);
    }
    return res;
  },
  rm = async (id) => {
    const res = await bidUserRm(id);
    USER_LI = USER_LI.filter(([uid]) => uid !== id);
    sync(USER_LI);
    return res;
  },
  mailSignin = async (mail, pwd) => {
    const res = await mailSigninApi(mail, pwd);
    if (res && res[0]) {
      await authRefresh();
    }
    return res;
  },
  userNewByMail = async (mail, name, pwd, code) => {
    const res = await userNewByMailApi(mail, name, pwd, code);
    if (res && res[0]) {
      await authRefresh();
    }
    return res;
  },
  mailChange = async (uid, mail, code_new, code_old) => {
    const res = await mailChangeApi(uid, mail, code_new, code_old);
    if (!res || !res[0]) {
      const u = USER_LI.find(([id]) => id === uid);
      if (u && u[3]) {
        u[3][1] = mail;
        sync(USER_LI);
      }
    }
    return res;
  };
