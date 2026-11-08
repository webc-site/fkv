# Cartão cápsula com seta para a direita exibida em suspensão

- Passe a seta para a direita
- Destaque azul ativo
- Clique para ampliar o feedback

## Use a demonstração

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Título 1</h2>
    <i>Descrição 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Título 2</h2>
    <i>Descrição 2</i>
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

## aula de estilo

- `.Card`: cartão básico
- `.Card.A`: Ativar destaque
- `> h2`: Título
- `> i`: descrição