# Kapsulas karte ar suspensijā attēlotu labo bultiņu

- Virziet kursoru pa labo bultiņu
- Aktīvs zils izgaismojums
- Noklikšķiniet, lai tuvinātu atsauksmes

## Izmantojiet demonstrāciju

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>1. nosaukums</h2>
    <i>Apraksts 1</i>
  </a>
  <a class="Card" href="#">
    <h2>2. nosaukums</h2>
    <i>2. apraksts</i>
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

## stila klase

- `.Card`: pamata karte
- `.Card.A`: aktivizējiet izcelšanu
- `> h2`: nosaukums
- `> i`: apraksts