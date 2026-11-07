# Tarjeta cápsula con flecha hacia la derecha mostrada en suspensión

- Pase el cursor sobre la flecha derecha
- Resaltado azul activo
- Haga clic para ampliar los comentarios

## Usa la demostración

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Título 1</h2>
    <i>Descripción 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Título 2</h2>
    <i>Descripción 2</i>
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

- `.Card`: Tarjeta básica
- `.Card.A`: Activar resaltado
- `> h2`: Título
- `> i`: descripción