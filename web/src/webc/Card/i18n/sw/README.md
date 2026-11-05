# Kadi ya kibonge yenye mshale wa kulia unaoonyeshwa kwa kusimamishwa

- Elea mshale wa kulia
- Kiangazio cha bluu kinachotumika
- Bofya ili kukuza maoni

## Tumia onyesho

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Kichwa cha 1</h2>
    <i>Maelezo 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Kichwa cha 2</h2>
    <i>Maelezo 2</i>
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

## darasa la mtindo

- `.Card`: Kadi ya msingi
- `.Card.A`: Washa uangaziaji
- `> h2`: Kichwa
- `> i`: maelezo