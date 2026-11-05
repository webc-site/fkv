# Targeta de la càpsula amb la fletxa dreta que es mostra en suspensió

- Passeu el cursor per la fletxa dreta
- Ressaltat blau actiu
- Feu clic per ampliar els comentaris

## Utilitzeu la demostració

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Títol 1</h2>
    <i>Descripció 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Títol 2</h2>
    <i>Descripció 2</i>
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

## classe d'estil

- `.Card`: targeta bàsica
- `.Card.A`: activa el ressaltat
- `> h2`: Títol
- `> i`: descripció