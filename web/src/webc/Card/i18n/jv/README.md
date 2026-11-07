# Kertu kapsul kanthi panah tengen ditampilake ing penundaan

- Arah panah tengen
- Sorotan biru aktif
- Klik kanggo nggedhekake umpan balik

## Gunakake demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Judhul 1</h2>
    <i>Katrangan 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Judhul 2</h2>
    <i>Katrangan 2</i>
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

## kelas gaya

- `.Card`: Kertu dhasar
- `.Card.A`: Aktifake sorotan
- `> h2`: Judul
- `> i`: katrangan