# Cerdyn capsiwl gyda saeth dde wedi'i harddangos yn yr ataliad

- Hofran y saeth dde
- Uchafbwynt glas gweithredol
- Cliciwch i chwyddo adborth

## Defnyddiwch y demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Teitl 1</h2>
    <i>Disgrifiad 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Teitl 2</h2>
    <i>Disgrifiad 2</i>
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

## dosbarth arddull

- `.Card`: Cerdyn sylfaenol
- `.Card.A`: Ysgogi amlygu
- `> h2`: Teitl
- `> i`: disgrifiad