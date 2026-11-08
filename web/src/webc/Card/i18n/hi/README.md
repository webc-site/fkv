# दाएँ तीर वाला कैप्सूल कार्ड निलंबन में प्रदर्शित होता है

- दाएँ तीर पर होवर करें
- सक्रिय नीला हाइलाइट
- फीडबैक को ज़ूम करने के लिए क्लिक करें

## डेमो का प्रयोग करें

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>शीर्षक 1</h2>
    <i>विवरण 1</i>
  </a>
  <a class="Card" href="#">
    <h2>शीर्षक 2</h2>
    <i>विवरण 2</i>
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

## शैली वर्ग

- `.Card`: मूल कार्ड
- `.Card.A`: हाइलाइटिंग सक्रिय करें
- <बी आईडी='1'>: शीर्षक
- <बी आईडी='1'>: विवरण