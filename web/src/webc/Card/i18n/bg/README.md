# Карта на капсула със стрелка надясно, показана в окачване

- Задръжте стрелката надясно
- Активно синьо осветяване
- Кликнете, за да увеличите обратната връзка

## Използвайте демонстрацията

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Заглавие 1</h2>
    <i>Описание 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Заглавие 2</h2>
    <i>Описание 2</i>
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

## стил клас

- `.Card`: Основна карта
- `.Card.A`: Активирайте осветяването
- `> h2`: Заглавие
- `> i`: описание