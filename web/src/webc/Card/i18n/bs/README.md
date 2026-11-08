# Kapsula kartica sa strelicom udesno prikazanom u suspenziji

- Zadržite strelicu udesno
- Aktivno plavo isticanje
- Kliknite za zumiranje povratnih informacija

## Koristite demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Naslov 1</h2>
    <i>Opis 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Naslov 2</h2>
    <i>Opis 2</i>
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

## stilska klasa

- `.Card`: Osnovna kartica
- `.Card.A`: Aktivirajte isticanje
- `> h2`: Naslov
- `> i`: opis