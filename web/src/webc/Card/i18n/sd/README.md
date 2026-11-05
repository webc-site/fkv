# ڪئپسول ڪارڊ ساڄي تير سان معطلي ۾ ڏيکاريل آهي

- ساڄي تير کي ڇڪيو
- فعال نيري نمايان
- تبصرو زوم ڪرڻ لاءِ ڪلڪ ڪريو

## ڊيمو استعمال ڪريو

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>عنوان 1</h2>
    <i>وضاحت 1</i>
  </a>
  <a class="Card" href="#">
    <h2>عنوان 2</h2>
    <i>وضاحت 2</i>
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

## انداز ڪلاس

- `.Card`: بنيادي ڪارڊ
- `.Card.A`: چالو ڪريو نمايان ڪرڻ
- `> h2`: عنوان
- `> i`: وضاحت