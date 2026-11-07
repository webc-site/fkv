# Tarxeta da cápsula coa frecha cara á dereita mostrada en suspensión

- Pasa o cursor da frecha cara á dereita
- Destacado azul activo
- Fai clic para ampliar os comentarios

## Use a demostración

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Título 1</h2>
    <i>Descrición 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Título 2</h2>
    <i>Descrición 2</i>
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

## clase de estilo

- `.Card`: tarxeta básica
- `.Card.A`: activa o resaltado
- `> h2`: Título
- `> i`: descrición