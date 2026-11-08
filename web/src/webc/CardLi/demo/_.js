import CardLi from "../../CardLi.js";
import { fetchLang } from "../../../js/fetchLang.js";
import i18nUrl, { COM, DESC } from "./I18N.js";

export default (root) => {
  const fLang = fetchLang(i18nUrl),
    [ing_container, empty_container, val_container] = ["ing", "empty", "val"].map((id) =>
      root.querySelector(".state." + id)
    );

  if (ing_container) {
    const ing_li = CardLi(() => new Promise(() => {}));
    ing_container.append(ing_li);
  }

  if (empty_container) {
    const empty_li = CardLi(async () => []);
    empty_container.append(empty_li);
  }

  if (val_container) {
    fLang((i18n) => {
      const val_li = CardLi(async () =>
        Array.from({ length: 8 }, (_, i) => {
          const n = i + 1;
          return [i18n[COM] + " " + n, i18n[DESC] + " " + n, "#" + n];
        })
      );
      val_container.replaceChildren(val_li);
    });
  }
};
