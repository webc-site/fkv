# Capsule card na may kanang arrow na ipinapakita sa pagsususpinde

- I-hover ang kanang arrow
- Aktibong asul na highlight
- I-click para mag-zoom ng feedback

## Gamitin ang demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Pamagat 1</h2>
    <i>Paglalarawan 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Pamagat 2</h2>
    <i>Paglalarawan 2</i>
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

## klase ng istilo

- `.Card`: Basic na card
- `.Card.A`: I-activate ang pag-highlight
- `> h2`: Pamagat
- `> i`: paglalarawan