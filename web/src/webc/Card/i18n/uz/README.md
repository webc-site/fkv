# O'ngga strelkali kapsula kartasi suspenziyada ko'rsatilgan

- O'ng strelkani suring
- Faol ko'k rang
- Fikr-mulohazalarni kattalashtirish uchun bosing

## Ko‘rsatma ishlatish

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Sarlavha 1</h2>
    <i>Tavsif 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Sarlavha 2</h2>
    <i>Tavsif 2</i>
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

## uslublar sinfi

- `.Card`: Asosiy karta
- `.Card.A`: ajratib ko'rsatishni faollashtirish
- `> h2`: Sarlavha
- `> i`: tavsif