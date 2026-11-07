# Оң жақ көрсеткісі бар капсула картасы суспензияда көрсетілген

- Оң жақ көрсеткіні апарыңыз
- Белсенді көк бояу
- Пікірді үлкейту үшін басыңыз

## Демонстрацияны пайдаланыңыз

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Тақырып 1</h2>
    <i>Сипаттама 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Тақырып 2</h2>
    <i>Сипаттама 2</i>
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

## стиль сыныбы

- `.Card`: Негізгі карта
- `.Card.A`: бөлектеуді белсендіру
- `> h2`: Тақырып
- `> i`: сипаттама