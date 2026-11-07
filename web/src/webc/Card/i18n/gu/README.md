# સસ્પેન્શનમાં પ્રદર્શિત જમણા તીર સાથે કૅપ્સ્યુલ કાર્ડ

- જમણા તીરને હૉવર કરો
- સક્રિય વાદળી હાઇલાઇટ
- પ્રતિસાદને ઝૂમ કરવા માટે ક્લિક કરો

## ડેમોનો ઉપયોગ કરો

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>શીર્ષક 1</h2>
    <i>વર્ણન 1</i>
  </a>
  <a class="Card" href="#">
    <h2>શીર્ષક 2</h2>
    <i>વર્ણન 2</i>
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

## શૈલી વર્ગ

- `.Card`: મૂળભૂત કાર્ડ
- `.Card.A`: હાઇલાઇટિંગ સક્રિય કરો
- `> h2`: શીર્ષક
- `> i`: વર્ણન