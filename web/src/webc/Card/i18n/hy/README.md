# Պարկուճային քարտ աջ սլաքով, որը ցուցադրվում է կասեցման մեջ

- Սավառնել աջ սլաքը
- Ակտիվ կապույտ ընդգծում
- Սեղմեք՝ մեծացնելու կարծիքը

## Օգտագործեք ցուցադրությունը

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Վերնագիր 1</h2>
    <i>Նկարագրություն 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Վերնագիր 2</h2>
    <i>Նկարագրություն 2</i>
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

## ոճի դաս

- `.Card`: Հիմնական քարտ
- `.Card.A`: Ակտիվացրեք ընդգծումը
- `> h2`՝ վերնագիր
- `> i`՝ նկարագրություն