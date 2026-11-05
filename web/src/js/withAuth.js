import { onAuth } from "./auth.js";
import { newEl } from "./dom.js";
import "../webc/Auth.js";

export default (host, onUser) => {
  let auth_el;

  const renderAuth = () => {
    if (!auth_el) {
      auth_el = newEl("c-auth");
      host.replaceChildren(auth_el);
    }
  };

  return onAuth((uid) => {
    if (uid) {
      if (auth_el) {
        auth_el.remove();
        auth_el = null;
      }
      onUser(uid);
    } else {
      renderAuth();
    }
  });
};
