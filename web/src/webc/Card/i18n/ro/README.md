# Cardul capsulei cu săgeata dreapta afișată în suspensie

- Treceți cu mouse-ul pe săgeata la dreapta
- Evidențiere albastră activă
- Faceți clic pentru a mări feedback-ul

## Utilizați demonstrația

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titlul 1</h2>
    <i>Descriere 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titlul 2</h2>
    <i>Descrierea 2</i>
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

## clasa de stil

- `.Card`: card de bază
- `.Card.A`: activați evidențierea
- `> h2`: Titlu
- `> i`: descriere