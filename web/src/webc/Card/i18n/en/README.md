# Capsule card with right arrow displayed in suspension

- Hover the right arrow
- Active blue highlight
- Click to zoom feedback

## Use the demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Title 1</h2>
    <i>Description 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Title 2</h2>
    <i>Description 2</i>
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

## Style Class

- `.Card`: Basic card
- `.Card.A`: Activate highlighting
- `> h2`: title
- `> i`: description