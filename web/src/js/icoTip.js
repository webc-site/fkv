import { cE, newLi } from "./dom.js";
import { fetchLang } from "./fetchLang.js";

export default (i18n_url, tag, key) => {
  const fLang = fetchLang(i18n_url);
  cE(
    tag,
    class extends HTMLElement {
      static get observedAttributes() {
        return ["href"];
      }

      attributeChangedCallback(name, old_val, new_val) {
        if (name === "href") {
          const btn = this.querySelector("a");
          if (btn) {
            btn.href = new_val || "";
          }
        }
      }

      connectedCallback() {
        if (this.firstChild) return;
        this.classList.add("Ico");

        const [pophover, btn, tip] = newLi("c-pophover", "a", "div"),
          href = this.getAttribute("href");

        if (href) {
          btn.href = href;
        }
        tip.slot = "pop";

        pophover.append(btn, tip);
        this.append(pophover);

        this.$ = fLang((i18n) => {
          tip.textContent = i18n[key];
        });
      }

      disconnectedCallback() {
        if (this.$) {
          this.$();
        }
      }
    }
  );
};
