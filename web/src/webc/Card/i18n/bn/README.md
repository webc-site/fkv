# সাসপেনশনে প্রদর্শিত ডান তীর সহ ক্যাপসুল কার্ড

- ডান তীরটি ঘোরান
- সক্রিয় নীল হাইলাইট
- প্রতিক্রিয়া জুম করতে ক্লিক করুন

## ডেমো ব্যবহার করুন

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>শিরোনাম 1</h2>
    <i>বর্ণনা ১</i>
  </a>
  <a class="Card" href="#">
    <h2>শিরোনাম 2</h2>
    <i>বর্ণনা 2</i>
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

## শৈলী ক্লাস

- `.Card`: মৌলিক কার্ড
- `.Card.A`: হাইলাইটিং সক্রিয় করুন৷
- `> h2`: শিরোনাম
- `> i`: বর্ণনা