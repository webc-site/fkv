# Картичка со капсула со десна стрелка прикажана во суспензија

- Движете ја десната стрелка
- Активно сино нагласување
- Кликнете за да зумирате повратни информации

## Користете го демо

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Наслов 1</h2>
    <i>Опис 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Наслов 2</h2>
    <i>Опис 2</i>
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

## класа стил

- `.Card`: Основна картичка
- `.Card.A`: Активирајте го истакнувањето
- `> h2`: Наслов
- `> i`: опис