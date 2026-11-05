# निलम्बन मा प्रदर्शित दायाँ तीर संग क्याप्सूल कार्ड

- दायाँ तीर होभर गर्नुहोस्
- सक्रिय नीलो हाइलाइट
- प्रतिक्रिया जुम गर्न क्लिक गर्नुहोस्

## डेमो प्रयोग गर्नुहोस्

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>शीर्षक १</h2>
    <i>विवरण १</i>
  </a>
  <a class="Card" href="#">
    <h2>शीर्षक २</h2>
    <i>विवरण २</i>
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

- `.Card`: आधारभूत कार्ड
- `.Card.A`: हाइलाइटिङ सक्रिय गर्नुहोस्
- `> h2`: शीर्षक
- `> i`: विवरण