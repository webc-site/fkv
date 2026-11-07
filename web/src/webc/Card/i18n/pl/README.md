# Karta kapsułki ze strzałką w prawo wyświetlaną w zawieszeniu

- Najedź prawą strzałką
- Aktywne niebieskie podświetlenie
- Kliknij, aby powiększyć opinię

## Skorzystaj z wersji demonstracyjnej

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Tytuł 1</h2>
    <i>Opis 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Tytuł 2</h2>
    <i>Opis 2</i>
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

## klasa stylu

- `.Card`: Karta podstawowa
- `.Card.A`: Włącz podświetlanie
- `> h2`: Tytuł
- `> i`: opis