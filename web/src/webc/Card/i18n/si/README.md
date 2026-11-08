# අත්හිටුවීම තුළ දකුණු ඊතලය සහිත කැප්සියුල කාඩ්පත

- දකුණු ඊතලය ගෙනයන්න
- ක්‍රියාකාරී නිල් උද්දීපනය
- ප්‍රතිපෝෂණ විශාලනය කිරීමට ක්ලික් කරන්න

## Demo භාවිතා කරන්න

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>මාතෘකාව 1</h2>
    <i>විස්තරය 1</i>
  </a>
  <a class="Card" href="#">
    <h2>මාතෘකාව 2</h2>
    <i>විස්තරය 2</i>
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

## ශෛලිය පන්තිය

- `.Card`: මූලික කාඩ්පත
- `.Card.A`: උද්දීපනය සක්‍රිය කරන්න
- `> h2`: මාතෘකාව
- `> i`: විස්තරය