# کارت کپسول با فلش سمت راست در حالت تعلیق نمایش داده می شود

- فلش سمت راست را نگه دارید
- هایلایت آبی فعال
- برای بزرگنمایی بازخورد کلیک کنید

## از دمو استفاده کنید

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>عنوان 1</h2>
    <i>توضیحات 1</i>
  </a>
  <a class="Card" href="#">
    <h2>عنوان 2</h2>
    <i>توضیحات 2</i>
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

## کلاس سبک

- `.Card`: کارت پایه
- `.Card.A`: برجسته سازی را فعال کنید
- `> h2`: عنوان
- `> i`: توضیحات