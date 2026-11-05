# Scheda capsula con freccia destra visualizzata in sospensione

- Passa il mouse sulla freccia destra
- Evidenziazione blu attiva
- Fare clic per ingrandire il feedback

## Utilizza la demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titolo 1</h2>
    <i>Descrizione 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titolo 2</h2>
    <i>Descrizione 2</i>
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

## lezione di stile

- `.Card`: Carta base
- `.Card.A`: attiva l'evidenziazione
- `> h2`: titolo
- `> i`: descrizione