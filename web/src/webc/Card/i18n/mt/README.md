# Kard tal-kapsula bil-vleġġa tal-lemin murija f'sospensjoni

- Passa l-vleġġa tal-lemin
- jenfasizza blu attiv
- Ikklikkja biex tkabbar il-feedback

## Uża d-demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titolu 1</h2>
    <i>Deskrizzjoni 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titolu 2</h2>
    <i>Deskrizzjoni 2</i>
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

## klassi tal-istil

- `.Card`: Karta bażika
- `.Card.A`: Attiva l-enfasi
- `> h2`: Titolu
- `> i`: deskrizzjoni