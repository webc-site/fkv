# Karta e kapsulës me shigjetën djathtas shfaqet në pezullim

- Rri pezull shigjetën e djathtë
- Theksim aktiv blu
- Kliko për të zmadhuar komentet

## Përdorni demonstrimin

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titulli 1</h2>
    <i>Përshkrimi 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titulli 2</h2>
    <i>Përshkrimi 2</i>
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

## klasë stili

- `.Card`: Karta bazë
- `.Card.A`: Aktivizo theksimin
- `> h2`: Titulli
- `> i`: përshkrim