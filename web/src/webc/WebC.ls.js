import "./Card.js";
import "./Kv.js";
import CardLi from "./CardLi.js";
import i18n, { CSS, BASE, WEBC, JS, GROUP } from "./WebC.ls/I18N.js";
import group from "./WebC.ls/group.js";
import "./WebC.ls/_.styl";
import { onLang } from "../js/i18n.js";
import { cE, newLi, newEl, clearDom } from "../js/dom.js";

const C_ING = "Ing";

cE(
  "webc-ls",
  class extends HTMLElement {
    connectedCallback() {
      const { classList } = this,
        unLang = onLang(async (lang) => {
          classList.add(C_ING);
          const [module, t] = await Promise.all([import(`../_/ls/${lang}.js`), i18n(lang)]),
            itemMap = ([component, desc]) => [component, desc, "/" + component],
            /*
          During packaging, keys will be optimized into numerical indices, while the property names of module remain original strings, so this array is introduced to decouple.
          打包时 keys 会被优化为数字索引，而 module 的属性名仍为原字符串，故引入此数组解耦
          */
            name_li = ["css", "base", "webc", "js"],
            key_li = [CSS, BASE, WEBC, JS],
            group_config = t[GROUP],
            groupMatch = group_config && group(group_config),
            sect_map = {},
            section_li = key_li.slice(0, -1).map((k) => {
              const sect = [k, t[k], [], false];
              sect_map[k] = sect;
              return sect;
            }),
            group_map = new Map();
          for (const [k, v] of Object.entries(group_config || {})) {
            const sect = [k, v, [], true];
            section_li.push(sect);
            group_map.set(v, sect);
          }

          const js_sect = [JS, t[JS], [], false];
          sect_map[JS] = js_sect;
          section_li.push(js_sect);

          key_li.forEach((k, i) => {
            (module[name_li[i]] || []).forEach((item) => {
              const [name] = item,
                target_gv = groupMatch && groupMatch(name),
                target_sect = (target_gv && group_map.get(target_gv)) || sect_map[k],
                [, , item_li] = target_sect;
              item_li.push(item);
            });
          });

          clearDom(this);

          section_li.forEach((sect) => {
            const [key_or_name, title, item_li, is_group] = sect;
            if (item_li.length > 0) {
              const [h2, bk] = newLi("h2", "b"),
                key_str = String(name_li[key_or_name] || key_or_name),
                id_val = "-" + (is_group ? key_str.match(/^\^([^(]+)/)[1] : key_str),
                a_el = newEl("a");
              h2.className = "O";
              h2.id = id_val;

              a_el.href = "#" + id_val;
              a_el.textContent = title;
              h2.replaceChildren(a_el);

              bk.className = "bk";
              if (key_str === "js") {
                const kv_el = newEl("b");
                kv_el.className = "KvLi";
                kv_el.append(
                  ...item_li.map(([name, desc]) => {
                    const [card, b_name, b_desc] = newLi("a", "b", "b");
                    card.href = "/js/" + name;
                    b_name.textContent = name;
                    b_desc.textContent = desc;
                    card.append(b_name, b_desc);
                    return card;
                  })
                );
                bk.replaceChildren(kv_el);
              } else {
                bk.replaceChildren(CardLi(async () => item_li.map(itemMap)));
              }

              this.append(h2, bk);
            }
          });

          classList.remove(C_ING);
          this.dispatchEvent(new Event("loaded", { bubbles: true }));
        });

      this.$ = () => {
        unLang();
      };
    }

    disconnectedCallback() {
      this.$();
    }
  }
);
