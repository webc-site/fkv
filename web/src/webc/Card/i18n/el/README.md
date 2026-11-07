# Κάρτα κάψουλας με δεξί βέλος που εμφανίζεται σε αναστολή

- Τοποθετήστε το δεξί βέλος
- Ενεργή μπλε επισήμανση
- Κάντε κλικ για μεγέθυνση στα σχόλια

## Χρησιμοποιήστε το demo

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>Τίτλος 1</h2>
    <i>Περιγραφή 1</i>
  </a>
  <a class="Card" href="#">
    <h2>Τίτλος 2</h2>
    <i>Περιγραφή 2</i>
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

## τάξη στυλ

- `.Card`: Βασική κάρτα
- `.Card.A`: Ενεργοποίηση επισήμανσης
- `> h2`: Τίτλος
- `> i`: περιγραφή