# Karta s kapsulou so šípkou doprava zobrazenou v závese

- Umiestnite kurzor myši na šípku doprava
- Aktívny modrý melír
- Kliknutím priblížite spätnú väzbu

## Použite demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Hlava 1</h2>
    <i>Popis 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Hlava 2</h2>
    <i>Popis 2</i>
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

## štýlová trieda

- `.Card`: Základná karta
- `.Card.A`: Aktivácia zvýraznenia
- `> h2`: Názov
- `> i`: popis