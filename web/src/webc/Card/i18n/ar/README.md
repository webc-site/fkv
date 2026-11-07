# بطاقة كبسولة مع سهم لليمين معروضة في التعليق

- قم بتمرير السهم الأيمن
- تسليط الضوء على اللون الأزرق النشط
- انقر لتكبير التعليقات

## استخدم العرض التوضيحي

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>العنوان 1</h2>
    <i>الوصف 1</i>
  </a>
  <a class="Card" href="#">
    <h2>العنوان 2</h2>
    <i>الوصف 2</i>
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

## فئة النمط

- `.Card`: البطاقة الأساسية
- `.Card.A`: تنشيط التمييز
- `> h2`: العنوان
- <b معرف = "1">: الوصف