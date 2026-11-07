# Kapszula kártya jobbra nyíllal, felfüggesztve

- Mutasson a jobbra mutató nyílra
- Aktív kék kiemelés
- Kattintson a nagyítás visszajelzéséhez

## Használd a demót

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>1. cím</h2>
    <i>Leírás 1</i>
  </a>
  <a class="Card" href="#">
    <h2>2. cím</h2>
    <i>Leírás 2</i>
  </a>
</main>

<script type="module">
import "webc.site/Card.js";
import { On } from "webc.site/js/On.js";

const el = document.querySelector("main"),
  CARD = ".Card",
  A = "A",
  CARD_A = CARD + "." + A;

On(el, {
  click: (e) => {
    const card = e.target.closest(CARD);
    if (card) {
      const active = el.querySelector(CARD_A);
      if (active && active !== card) {
        active.classList.remove(A);
      }
      card.classList.add(A);
    }
  },
});
</script>
```

## stílus osztály

- `.Card`: Alapkártya
- `.Card.A`: Kiemelés aktiválása
- `> h2`: Cím
- `> i`: leírás