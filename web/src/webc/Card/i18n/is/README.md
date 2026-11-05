# Hylkisspjald með hægri ör sem birtist í dreifu

- Snúðu hægri örina
- Virkur blár hápunktur
- Smelltu til að stækka viðbrögð

## Notaðu kynninguna

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titill 1</h2>
    <i>Lýsing 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titill 2</h2>
    <i>Lýsing 2</i>
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

## stílaflokkur

- `.Card`: Grunnkort
- `.Card.A`: Virkjaðu auðkenningu
- `> h2`: Titill
- `> i`: lýsing