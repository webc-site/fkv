# ဆိုင်းထိန်းစနစ်တွင် ညာဘက်မြှားပြထားသည့် Capsule ကတ်

- ညာဘက်မြှားကို ရွှေ့ပါ။
- အသက်ဝင်သော အပြာရောင်အသားပေး
- အကြံပြုချက်ကို ချုံ့ချဲ့ရန် နှိပ်ပါ။

## ဒီမိုကိုသုံးပါ။

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>ခေါင်းစဉ် ၁</h2>
    <i>ဖော်ပြချက် ၁</i>
  </a>
  <a class="Card" href="#">
    <h2>ခေါင်းစဉ် ၂</h2>
    <i>ဖော်ပြချက် ၂</i>
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

## စတိုင်အတန်း

- `.Card`- အခြေခံကတ်
- `.Card.A`- မီးမောင်းထိုးပြခြင်းကို အသက်သွင်းပါ။
- `> h2`- ခေါင်းစဉ်
- `> i`- ဖော်ပြချက်