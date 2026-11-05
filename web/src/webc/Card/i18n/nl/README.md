# Capsulekaart met pijl naar rechts weergegeven in suspensie

- Beweeg de rechterpijl
- Actief blauw hoogtepunt
- Klik om de feedback in te zoomen

## Gebruik de demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titel 1</h2>
    <i>Beschrijving 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titel 2</h2>
    <i>Beschrijving 2</i>
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

## stijl klasse

- `.Card`: Basiskaart
- `.Card.A`: Markering activeren
- `> h2`: Titel
- `> i`: beschrijving