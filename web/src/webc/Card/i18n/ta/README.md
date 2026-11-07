# வலது அம்புக்குறியுடன் கூடிய கேப்சூல் அட்டை இடைநீக்கத்தில் காட்டப்பட்டுள்ளது

- வலது அம்புக்குறியை வட்டமிடுங்கள்
- செயலில் நீல ஹைலைட்
- கருத்தை பெரிதாக்க கிளிக் செய்யவும்

## டெமோவைப் பயன்படுத்தவும்

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>தலைப்பு 1</h2>
    <i>விளக்கம் 1</i>
  </a>
  <a class="Card" href="#">
    <h2>தலைப்பு 2</h2>
    <i>விளக்கம் 2</i>
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

## பாணி வகுப்பு

- `.Card`: அடிப்படை அட்டை
- `.Card.A`: தனிப்படுத்தலைச் செயல்படுத்தவும்
- `> h2`: தலைப்பு
- `> i`: விளக்கம்