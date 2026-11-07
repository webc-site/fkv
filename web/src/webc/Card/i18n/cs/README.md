# Karta kapsle se šipkou vpravo zobrazená v zavěšení

- Najeďte na šipku vpravo
- Aktivní modrý melír
- Kliknutím přiblížíte zpětnou vazbu

## Použijte demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Titul 1</h2>
    <i>Popis 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Titul 2</h2>
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

## stylová třída

- `.Card`: Základní karta
- `.Card.A`: Aktivovat zvýraznění
- `> h2`: Název
- `> i`: popis