# Eskuineko gezia esekituta ageri den kapsularen txartela

- Pasatu eskuineko gezia
- Nabarmendu urdin aktiboa
- Egin klik iritzia handitzeko

## Erabili demoa

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>1. titulua</h2>
    <i>Deskribapena 1</i>
  </a>
  <a class="Card" href="#">
    <h2>2. titulua</h2>
    <i>Deskribapena 2</i>
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

## estilo klasea

- `.Card`: oinarrizko txartela
- `.Card.A`: aktibatu nabarmentzea
- `> h2`: Izenburua
- `> i`: deskribapena