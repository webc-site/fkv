# כרטיס קפסולה עם חץ ימינה מוצג בהשעיה

- העבר את החץ ימינה
- הדגשה כחולה פעילה
- לחץ כדי להגדיל משוב

## השתמש בהדגמה

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>כותרת 1</h2>
    <i>תיאור 1</i>
  </a>
  <a class="Card" href="#">
    <h2>כותרת 2</h2>
    <i>תיאור 2</i>
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

## שיעור סגנון

- `.Card`: כרטיס בסיסי
- `.Card.A`: הפעל הדגשה
- `> h2`: כותרת
- `> i`: תיאור