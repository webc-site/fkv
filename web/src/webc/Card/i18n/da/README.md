# Kapselkort med højre pil vist i suspension

- Hold musen over den højre pil
- Aktiv blå highlight
- Klik for at zoome feedback

## Brug demoen

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titel 1</h2>
    <i>Beskrivelse 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titel 2</h2>
    <i>Beskrivelse 2</i>
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

## stil klasse

- `.Card`: Grundlæggende kort
- `.Card.A`: Aktiver fremhævning
- `> h2`: Titel
- `> i`: beskrivelse