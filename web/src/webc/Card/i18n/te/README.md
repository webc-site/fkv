# సస్పెన్షన్‌లో ప్రదర్శించబడిన కుడి బాణంతో క్యాప్సూల్ కార్డ్

- కుడి బాణాన్ని హోవర్ చేయండి
- యాక్టివ్ బ్లూ హైలైట్
- అభిప్రాయాన్ని జూమ్ చేయడానికి క్లిక్ చేయండి

## డెమో ఉపయోగించండి

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>శీర్షిక 1</h2>
    <i>వివరణ 1</i>
  </a>
  <a class="Card" href="#">
    <h2>శీర్షిక 2</h2>
    <i>వివరణ 2</i>
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

## శైలి తరగతి

- `.Card`: ప్రాథమిక కార్డ్
- `.Card.A`: హైలైట్ చేయడాన్ని సక్రియం చేయండి
- `> h2`: శీర్షిక
- `> i`: వివరణ