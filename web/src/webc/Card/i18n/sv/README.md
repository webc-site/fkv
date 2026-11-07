# Kapselkort med högerpil visas i suspension

- Håll muspekaren över högerpilen
- Aktiv blå höjdpunkt
- Klicka för att zooma feedback

## Använd demon

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Rubrik 1</h2>
    <i>Beskrivning 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Rubrik 2</h2>
    <i>Beskrivning 2</i>
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

## stilklass

- `.Card`: Grundkort
- `.Card.A`: Aktivera markering
- `> h2`: Titel
- `> i`: beskrivning