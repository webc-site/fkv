# Карточка-капсула со стрелкой вправо отображается в подвешенном состоянии.

- Наведите курсор на стрелку вправо
- Активная синяя подсветка
- Нажмите, чтобы увеличить масштаб отзыва

## Используйте демо-версию

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Название 1</h2>
    <i>Описание 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Название 2</h2>
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

## класс стиля

- `.Card`: базовая карта.
- `.Card.A`: активировать выделение
- `> h2`: Название
- `> i`: описание