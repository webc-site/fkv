import "../../TabSelect.js";
import "../../Select.js";
import { newLi, newEl } from "../../../js/dom.js";
import "../../Copy.js";
import { ONLINE, LOCAL } from "../I18N.js";
import { HOST_LI, jsdUrl, PKG_VER } from "./JSD.js";
import { On } from "../../../js/On.js";

const CLI = "@webc.site/cli@latest add ",
  KEY = "webcDemoUse",
  PKG_CMD = {
    bun: "bun x " + CLI,
    npx: "npx " + CLI,
    deno: "deno run -A npm:" + CLI
  },
  A = 0,
  PRE = 1,
  VAL = 2,
  SET_TXT = 3,
  SET_LANG = 4;

export default () => {
  const makeTab = (type, options_li) => {
      const [a, pre, code] = newLi("a", "b", "code"),
        span = newEl("span"),
        select = newEl("c-select"),
        copy = newEl("c-copy");

      options_li.forEach((v) => {
        const opt = newEl("option");
        opt.value = opt.textContent = v;
        select.append(opt);
      });

      a.setAttribute("value", type);
      a.append(span, select);

      pre.setAttribute("slot", type);
      pre.append(code, copy);

      return [
        a,
        pre,
        () => select.value,
        (txt) => {
          code.textContent = txt;
        },
        (txt) => {
          span.textContent = txt;
        }
      ];
    },
    online = makeTab("online", HOST_LI),
    local = makeTab("local", ["bun", "npx", "deno"]),
    [tab_el, tab_nav] = newLi("c-tab-select", "nav");

  let current_name = "";

  tab_el.setAttribute("key", KEY);
  tab_el.setAttribute("value", "online/" + HOST_LI[0]);

  tab_nav.append(online[A], local[A]);
  tab_el.append(tab_nav, online[PRE], local[PRE]);

  const updateDisplay = () => {
      if (!current_name) return;

      const host = online[VAL](),
        pkg = local[VAL](),
        jsd = jsdUrl(host);

      online[SET_TXT]('import "' + jsd + current_name + '.js"');
      local[SET_TXT]((PKG_CMD[pkg] || "") + PKG_VER + "/" + current_name);
    },
    render = (name) => {
      current_name = name;
      updateDisplay();
    },
    updateLang = (i18n) => {
      online[SET_LANG](i18n[ONLINE]);
      local[SET_LANG](i18n[LOCAL]);
    };

  On(tab_el, { change: updateDisplay });

  return [tab_el, render, updateLang];
};
