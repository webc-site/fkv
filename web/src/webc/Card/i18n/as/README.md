# সোঁ কাঁড় চিহ্নৰ সৈতে কেপচুল কাৰ্ড ঝুলনত প্ৰদৰ্শিত

- সোঁ কাঁড় চিহ্নটো হুভাৰ কৰক
- সক্ৰিয় নীলা হাইলাইট
- মতামত জুম কৰিবলৈ ক্লিক কৰক

## ডেমো ব্যৱহাৰ কৰক

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>শিৰোনাম ১</h2>
    <i>বিৱৰণ 1</i>
  </a>
  <a class="Card" href="#">
    <h2>শিৰোনাম ২</h2>
    <i>বিৱৰণ 2</i>
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

## শৈলী শ্ৰেণী

- `.Card`: মৌলিক কাৰ্ড
- `.Card.A`: হাইলাইটিং সক্ৰিয় কৰক
- `> h2`: শিৰোনাম
- `> i`: বিৱৰণ