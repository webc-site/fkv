# Kapselkort med høyre pil vist i suspensjon

- Hold musepekeren til høyre
- Aktivt blått høydepunkt
- Klikk for å zoome tilbakemelding

## Bruk demoen

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Tittel 1</h2>
    <i>Beskrivelse 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Tittel 2</h2>
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

## stilklasse

- `.Card`: Grunnleggende kort
- `.Card.A`: Aktiver utheving
- `> h2`: Tittel
- `> i`: beskrivelse