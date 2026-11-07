# Картка капсулы са стрэлкай управа, якая адлюстроўваецца ў падвесцы

- Навядзіце курсор направа
- Актыўная сіняя падсветка
- Націсніце, каб павялічыць водгук

## Выкарыстоўвайце дэма

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Назва 1</h2>
    <i>Апісанне 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Назва 2</h2>
    <i>Апісанне 2</i>
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

## клас стылю

- `.Card`: Асноўная карта
- `.Card.A`: Актываваць вылучэнне
- `> h2`: Назва
- `> i`: апісанне