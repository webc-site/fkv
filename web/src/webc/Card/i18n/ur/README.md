# دائیں تیر کے ساتھ کیپسول کارڈ معطلی میں دکھایا گیا ہے۔

- دائیں تیر کو ہوور کریں۔
- ایکٹو بلیو ہائی لائٹ
- تاثرات کو زوم کرنے کے لیے کلک کریں۔

## ڈیمو استعمال کریں۔

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>عنوان 1</h2>
    <i>تفصیل 1</i>
  </a>
  <a class="Card" href="#">
    <h2>عنوان 2</h2>
    <i>تفصیل 2</i>
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

## سٹائل کلاس

- `.Card`: بنیادی کارڈ
- `.Card.A`: ہائی لائٹنگ کو فعال کریں۔
- `> h2`: عنوان
- `> i`: تفصیل